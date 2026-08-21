use crate::{
    error::{KfResult, LocalizedError},
    project::{canonical_root, refresh_indexed_file, resolve_indexed_tool_path, resolve_inside},
    state::{AppState, IndexedProject},
};
use serde::Serialize;
use std::{
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Instant,
};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::sync::CancellationToken;

const MAX_READ_LINES: usize = 800;
const MAX_OUTPUT_BYTES: usize = 128 * 1024;
const OUTPUT_HEAD_BYTES: usize = 32 * 1024;
const OUTPUT_OMISSION_MARKER: &[u8] = b"\n... output omitted ...\n";

#[cfg(windows)]
struct ProcessJob(isize);

#[cfg(windows)]
impl ProcessJob {
    fn assign(child: &tokio::process::Child) -> std::io::Result<Self> {
        use windows_sys::Win32::{
            Foundation::GetLastError,
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
        };
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::from_raw_os_error(unsafe {
                GetLastError() as i32
            }));
        }
        let job = Self(handle as isize);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job.0 as windows_sys::Win32::Foundation::HANDLE,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if configured == 0 {
            return Err(std::io::Error::from_raw_os_error(unsafe {
                GetLastError() as i32
            }));
        }
        let process = child
            .raw_handle()
            .ok_or_else(|| std::io::Error::other("process handle unavailable"))?
            as windows_sys::Win32::Foundation::HANDLE;
        if unsafe {
            AssignProcessToJobObject(job.0 as windows_sys::Win32::Foundation::HANDLE, process)
        } == 0
        {
            return Err(std::io::Error::from_raw_os_error(unsafe {
                GetLastError() as i32
            }));
        }
        Ok(job)
    }

    fn terminate(&self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(
                self.0 as windows_sys::Win32::Foundation::HANDLE,
                1,
            );
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(
                self.0 as windows_sys::Win32::Foundation::HANDLE,
            );
        }
    }
}
pub fn read_for_agent(
    state: &AppState,
    root: &str,
    path: &str,
    start: usize,
    end: usize,
) -> KfResult<ReadResult> {
    let root = project_root(state, Some(root))?;
    let path = resolve_indexed_tool_path(state, &root, path, true)?;
    read_range(&path, start, end)
}

pub fn edit_for_agent(
    state: &AppState,
    root: &str,
    path: &str,
    old: &str,
    new: &str,
) -> KfResult<EditResult> {
    let root = project_root(state, Some(root))?;
    let path = resolve_indexed_tool_path(state, &root, path, true)?;
    let result = edit_exact(&path, old, new)?;
    refresh_indexed_file(state, &root, &path);
    Ok(result)
}

/// Whole-file write: create a file or replace its entire contents. Distinct
/// from `edit` (exact unique fragment replacement) — rewriting a broken file
/// or creating a new one through `edit` forces whole-file oldText payloads
/// that are long, fragile, and error-prone.
pub fn write_for_agent(state: &AppState, root: &str, path: &str, content: &str) -> KfResult<u64> {
    let root = project_root(state, Some(root))?;
    // Graph resolution serves existing files; for a brand-new file in a
    // not-yet-existing directory it fails on the missing parent — fall back
    // to a direct root-relative join, then create the directory.
    let path = match resolve_indexed_tool_path(state, &root, path, false) {
        Ok(resolved) => resolved,
        Err(error) if error.key == "error.path_parent" => {
            let relative = path.trim().replace('\\', "/");
            root.join(relative)
        }
        Err(error) => return Err(error),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            LocalizedError::new("error.edit_write").arg("detail", format!("{parent:?}: {e}"))
        })?;
    }
    std::fs::write(&path, content)
        .map_err(|e| LocalizedError::new("error.edit_write").arg("detail", e))?;
    let bytes = content.len() as u64;
    refresh_indexed_file(state, &root, &path);
    Ok(bytes)
}

pub async fn run_for_agent(
    state: &AppState,
    root: &str,
    program: String,
    args: Vec<String>,
    cancellation: &tokio_util::sync::CancellationToken,
) -> KfResult<RunResult> {
    let root = project_root(state, Some(root))?;
    let (program, args) = normalize_invocation(&root, program, args)?;
    let advisory = exploration_advisory(&program, &args);
    run_normalized(root, program, args, advisory, cancellation).await
}

pub async fn run_command_for_agent(
    state: &AppState,
    root: &str,
    command: String,
    cancellation: &CancellationToken,
) -> KfResult<RunResult> {
    let root = project_root(state, Some(root))?;
    #[cfg(windows)]
    let (program, args) = (
        "powershell.exe".to_owned(),
        vec![
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            command,
        ],
    );
    #[cfg(not(windows))]
    let (program, args) = ("sh".to_owned(), vec!["-c".into(), command]);
    let advisory = exploration_advisory(&program, &args);
    run_normalized(root, program, args, advisory, cancellation).await
}

async fn run_in_root(
    root: PathBuf,
    program: String,
    args: Vec<String>,
    cancellation: &CancellationToken,
) -> KfResult<RunResult> {
    let (program, args) = normalize_invocation(&root, program, args)?;
    run_normalized(root, program, args, None, cancellation).await
}

async fn run_normalized(
    root: PathBuf,
    program: String,
    args: Vec<String>,
    advisory: Option<String>,
    cancellation: &CancellationToken,
) -> KfResult<RunResult> {
    reject_real_transaction(&program, &args)?;
    let requested_program = program.clone();
    let (program, args) = prepare_command(&root, program, args);
    let mut command = tokio::process::Command::new(&program);
    command
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    let started = Instant::now();
    let mut child = command.spawn().map_err(|e| {
        LocalizedError::new("error.run_spawn")
            .arg("program", &requested_program)
            .arg("detail", e)
    })?;
    #[cfg(windows)]
    let process_job = ProcessJob::assign(&child).ok();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| LocalizedError::new("error.run_pipe"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| LocalizedError::new("error.run_pipe"))?;
    let stdout_task = tokio::spawn(read_limited(stdout));
    let stderr_task = tokio::spawn(read_limited(stderr));
    let status = tokio::select! {
        _ = cancellation.cancelled() => {
            #[cfg(windows)]
            if let Some(job) = &process_job { job.terminate(); }
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(LocalizedError::new("error.session_cancelled"));
        },
        status = child.wait() => status.map_err(|e| LocalizedError::new("error.run_wait").arg("detail", e))?,
    };
    #[cfg(windows)]
    if let Some(job) = &process_job {
        job.terminate();
    }
    let stdout = join_output(stdout_task).await?;
    let stderr = join_output(stderr_task).await?;
    Ok(output_result(status, stdout, stderr, started, advisory))
}

fn normalize_invocation(
    root: &Path,
    program: String,
    args: Vec<String>,
) -> KfResult<(String, Vec<String>)> {
    let mut program = program.trim().to_owned();
    if program.len() >= 2 {
        let quoted = (program.starts_with('"') && program.ends_with('"'))
            || (program.starts_with('\'') && program.ends_with('\''));
        if quoted {
            program = program[1..program.len() - 1].to_owned();
        }
    }
    if program.is_empty() {
        return Err(LocalizedError::new("error.run_program"));
    }
    let candidate = Path::new(&program);
    let existing_path = candidate.is_absolute() && candidate.is_file()
        || (!candidate.is_absolute() && root.join(candidate).is_file());
    if existing_path || !program.chars().any(char::is_whitespace) {
        return Ok((program, args));
    }

    let mut words = split_command_line(&program)?;
    if words.is_empty() {
        return Err(LocalizedError::new("error.run_program"));
    }
    let executable = words.remove(0);
    words.extend(args);
    Ok((executable, words))
}

pub(crate) fn split_command_line(input: &str) -> KfResult<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut chars = input.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\\'
            && quote.is_some()
            && chars.peek().is_some_and(|next| Some(*next) == quote)
        {
            current.push(chars.next().expect("peeked quote"));
            continue;
        }
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            } else {
                current.push(character);
            }
        } else if character.is_whitespace() && quote.is_none() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if quote.is_some() {
        return Err(LocalizedError::new("error.tool_argument")
            .arg("field", "command")
            .arg("detail", "unclosed quote"));
    }
    if !current.is_empty() {
        words.push(current);
    }
    Ok(words)
}

pub(crate) const EXPLORATION_ADVISORY: &str =
    "Prefer the project graph plus find/search/read/edit over shell project discovery";

fn exploration_advisory(program: &str, args: &[String]) -> Option<String> {
    invocation_uses_project_enumeration(program, args, 0).then(|| EXPLORATION_ADVISORY.to_owned())
}

const MAX_SHELL_INSPECTION_DEPTH: usize = 8;

fn invocation_uses_project_enumeration(program: &str, args: &[String], depth: usize) -> bool {
    if depth >= MAX_SHELL_INSPECTION_DEPTH {
        return true;
    }
    let command = executable_name(program);
    if is_project_enumerator(&command) {
        return true;
    }
    if interpreter_script(&command, args).is_some_and(script_touches_project_files) {
        return true;
    }
    let script = match command.as_str() {
        "powershell" | "pwsh" => {
            if powershell_uses_encoded_command(args) {
                return true;
            }
            powershell_script(args)
        }
        "cmd" => cmd_script(args),
        "bash" | "sh" | "zsh" => unix_shell_script(args),
        _ => None,
    };
    if script.is_some_and(|script| script_uses_project_enumeration(&script, depth + 1)) {
        return true;
    }
    if is_command_launcher(&command)
        && let Some((nested_program, nested_args)) = launcher_target(&command, args)
    {
        return invocation_uses_project_enumeration(nested_program, nested_args, depth + 1);
    }
    false
}

fn is_project_enumerator(command: &str) -> bool {
    matches!(
        command,
        "cat"
            | "dir"
            | "fd"
            | "fdfind"
            | "find"
            | "findstr"
            | "gc"
            | "get-childitem"
            | "get-content"
            | "gci"
            | "grep"
            | "head"
            | "less"
            | "locate"
            | "ls"
            | "more"
            | "rg"
            | "select-string"
            | "tail"
            | "tree"
            | "type"
            | "where"
    )
}

fn interpreter_script<'a>(command: &str, args: &'a [String]) -> Option<&'a str> {
    let flag = match command {
        "node" | "nodejs" | "ruby" => "-e",
        "python" | "python3" | "py" => "-c",
        _ => return None,
    };
    args.iter()
        .position(|argument| argument.eq_ignore_ascii_case(flag))
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn script_touches_project_files(script: &str) -> bool {
    let script = script.to_ascii_lowercase();
    [
        "fs.readdir",
        "fs.readfile",
        "fs.writefile",
        "glob(",
        "glob.glob",
        "open(",
        "os.listdir",
        "os.scandir",
        "os.walk",
        "pathlib",
        "read_text(",
        "readdir(",
        "write_text(",
    ]
    .iter()
    .any(|needle| script.contains(needle))
}

fn executable_name(program: &str) -> String {
    let program = program
        .trim()
        .trim_matches(['\'', '"'])
        .trim_start_matches('@');
    Path::new(program)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase()
}

fn powershell_uses_encoded_command(args: &[String]) -> bool {
    args.iter().any(|argument| {
        let argument = argument.to_ascii_lowercase();
        argument.len() >= 2
            && ["-encodedcommand", "-encodedarguments"]
                .iter()
                .any(|full| full.starts_with(&argument))
    })
}

fn powershell_script(args: &[String]) -> Option<String> {
    args.iter().enumerate().find_map(|(index, argument)| {
        let argument = argument.to_ascii_lowercase();
        let is_command = argument == "-c"
            || (argument.len() >= 3 && "-command".starts_with(&argument))
            || argument == "-commandwithargs";
        is_command.then(|| args[index + 1..].join(" "))
    })
}

fn cmd_script(args: &[String]) -> Option<String> {
    args.iter().enumerate().find_map(|(index, argument)| {
        let argument_lower = argument.to_ascii_lowercase();
        if matches!(argument_lower.as_str(), "/c" | "/k") {
            return Some(args[index + 1..].join(" "));
        }
        if (argument_lower.starts_with("/c") || argument_lower.starts_with("/k"))
            && argument.len() > 2
        {
            let mut script = argument[2..].to_owned();
            if index + 1 < args.len() {
                script.push(' ');
                script.push_str(&args[index + 1..].join(" "));
            }
            return Some(script);
        }
        None
    })
}

fn unix_shell_script(args: &[String]) -> Option<String> {
    args.iter().enumerate().find_map(|(index, argument)| {
        let is_command = argument.starts_with('-')
            && !argument.starts_with("--")
            && argument[1..].chars().any(|flag| flag == 'c');
        is_command.then(|| args.get(index + 1).cloned()).flatten()
    })
}

fn script_uses_project_enumeration(script: &str, depth: usize) -> bool {
    let script = script.to_ascii_lowercase();
    if script.contains("get-childitem") || script.contains("select-string") {
        return true;
    }
    script
        .split([';', '|', '&', '\n', '\r', '(', ')', '{', '}', '`'])
        .any(|segment| {
            let segment = segment.trim_start_matches([' ', '\t', '@']);
            let words = split_command_line(segment).unwrap_or_else(|_| {
                segment
                    .split_whitespace()
                    .map(|word| word.trim_matches(['\'', '"']).to_owned())
                    .collect()
            });
            words.first().is_some_and(|program| {
                invocation_uses_project_enumeration(program, &words[1..], depth)
            })
        })
}

fn is_command_launcher(command: &str) -> bool {
    matches!(
        command,
        "!" | "builtin"
            | "busybox"
            | "call"
            | "command"
            | "do"
            | "elif"
            | "else"
            | "env"
            | "exec"
            | "if"
            | "ionice"
            | "nice"
            | "nohup"
            | "parallel"
            | "start"
            | "stdbuf"
            | "sudo"
            | "then"
            | "time"
            | "until"
            | "watch"
            | "while"
            | "wsl"
            | "xargs"
    )
}

fn launcher_target<'a>(command: &str, args: &'a [String]) -> Option<(&'a str, &'a [String])> {
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--" {
            index += 1;
            break;
        }
        if argument.contains('=') && !argument.starts_with('=') {
            index += 1;
            continue;
        }
        if argument.starts_with('-') || (command == "start" && argument.starts_with('/')) {
            index += 1;
            if launcher_option_takes_value(command, argument) && index < args.len() {
                index += 1;
            }
            continue;
        }
        break;
    }
    args.get(index)
        .map(|program| (program.as_str(), &args[index + 1..]))
}

fn launcher_option_takes_value(command: &str, argument: &str) -> bool {
    match command {
        "env" => matches!(
            argument,
            "-u" | "--unset" | "-C" | "--chdir" | "-S" | "--split-string"
        ),
        "sudo" => matches!(
            argument,
            "-u" | "--user" | "-g" | "--group" | "-h" | "--host" | "-p" | "--prompt"
        ),
        "xargs" => matches!(argument, "-a" | "-E" | "-I" | "-L" | "-n" | "-P" | "-s"),
        "nice" => argument == "-n",
        "ionice" => matches!(argument, "-c" | "-n" | "-p" | "-P" | "-u"),
        "stdbuf" => matches!(argument, "-i" | "-o" | "-e"),
        "watch" => matches!(argument, "-n" | "--interval"),
        _ => false,
    }
}

fn prepare_command(root: &Path, program: String, args: Vec<String>) -> (String, Vec<String>) {
    #[cfg(windows)]
    if let Some(path) = resolve_windows_program(root, &program) {
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ps1"))
        {
            let mut powershell_args = vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                path.display().to_string(),
            ];
            powershell_args.extend(args);
            return ("powershell.exe".into(), powershell_args);
        }
        return (path.display().to_string(), args);
    }
    let _ = root;
    (program, args)
}

#[cfg(windows)]
fn resolve_windows_program(root: &Path, program: &str) -> Option<PathBuf> {
    let input = Path::new(program);
    let mut bases = Vec::new();
    if input.is_absolute() {
        bases.push(input.to_path_buf());
    } else if input.components().count() > 1 {
        bases.push(root.join(input));
    } else {
        bases.push(root.join(input));
        if let Some(path) = std::env::var_os("PATH") {
            bases.extend(std::env::split_paths(&path).map(|directory| directory.join(input)));
        }
    }

    let extensions: Vec<String> = if input.extension().is_some() {
        vec![String::new()]
    } else {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(str::to_owned)
            .chain(std::iter::once(".ps1".into()))
            .collect()
    };
    for base in bases {
        for extension in &extensions {
            let candidate = if extension.is_empty() {
                base.clone()
            } else {
                PathBuf::from(format!("{}{}", base.display(), extension))
            };
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn reject_real_transaction(program: &str, args: &[String]) -> KfResult<()> {
    let command = format!(" {} {} ", program, args.join(" ")).to_ascii_lowercase();
    let financial_target = [
        "binance",
        "coinbase",
        "kraken",
        "alpaca",
        "interactivebrokers",
        "ibkr",
        "broker",
        "trading",
        "checkout",
        "payment",
    ]
    .iter()
    .any(|term| command.contains(term));
    let mutation = [
        " buy ",
        " sell ",
        " order",
        " trade",
        " position",
        " purchase",
        " checkout",
        " payment",
    ]
    .iter()
    .any(|term| command.contains(term));
    if financial_target && mutation {
        Err(LocalizedError::new("error.transaction_forbidden"))
    } else {
        Ok(())
    }
}

struct CapturedOutput {
    bytes: Vec<u8>,
    total_bytes: usize,
    truncated: bool,
}

async fn read_limited<R: AsyncRead + Unpin>(mut reader: R) -> std::io::Result<CapturedOutput> {
    let tail_capacity = MAX_OUTPUT_BYTES
        .saturating_sub(OUTPUT_HEAD_BYTES)
        .saturating_sub(OUTPUT_OMISSION_MARKER.len());
    let mut head = Vec::with_capacity(OUTPUT_HEAD_BYTES);
    let mut tail = Vec::with_capacity(tail_capacity);
    let mut buffer = [0_u8; 8192];
    let mut total_bytes = 0usize;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(read);
        let head_copy = read.min(OUTPUT_HEAD_BYTES.saturating_sub(head.len()));
        head.extend_from_slice(&buffer[..head_copy]);
        if head_copy < read {
            tail.extend_from_slice(&buffer[head_copy..read]);
            if tail.len() > tail_capacity {
                tail.drain(..tail.len() - tail_capacity);
            }
        }
    }
    let truncated = total_bytes > MAX_OUTPUT_BYTES;
    let mut bytes = head;
    if truncated {
        bytes.extend_from_slice(OUTPUT_OMISSION_MARKER);
    }
    bytes.extend_from_slice(&tail);
    Ok(CapturedOutput {
        bytes,
        total_bytes,
        truncated,
    })
}

async fn join_output(
    task: tokio::task::JoinHandle<std::io::Result<CapturedOutput>>,
) -> KfResult<CapturedOutput> {
    task.await
        .map_err(|error| LocalizedError::new("error.run_pipe").arg("detail", error))?
        .map_err(|error| LocalizedError::new("error.run_pipe").arg("detail", error))
}

fn output_result(
    status: std::process::ExitStatus,
    stdout: CapturedOutput,
    stderr: CapturedOutput,
    started: Instant,
    advisory: Option<String>,
) -> RunResult {
    RunResult {
        exit_code: status.code(),
        elapsed_ms: started.elapsed().as_millis(),
        stdout: String::from_utf8_lossy(&stdout.bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr.bytes).into_owned(),
        stdout_bytes: stdout.total_bytes,
        stderr_bytes: stderr.total_bytes,
        truncated: stdout.truncated || stderr.truncated,
        advisory,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResult {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    pub path: String,
    pub replacements: usize,
    pub bytes_written: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub exit_code: Option<i32>,
    pub elapsed_ms: u128,
    pub stdout: String,
    pub stderr: String,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub total: usize,
    pub truncated: bool,
}

fn project_root(state: &AppState, root: Option<&str>) -> KfResult<PathBuf> {
    if let Some(root) = root {
        let path = Path::new(root);
        if path.is_absolute() {
            return canonical_root(path);
        }
        if let Some(project) = state.active_project.read().as_ref() {
            return canonical_root(&project.join(path));
        }
        return canonical_root(path);
    }
    state
        .active_project
        .read()
        .clone()
        .ok_or_else(|| LocalizedError::new("error.project_none"))
}

pub fn read_range(path: &Path, start_line: usize, end_line: usize) -> KfResult<ReadResult> {
    if start_line == 0 || end_line < start_line || end_line - start_line + 1 > MAX_READ_LINES {
        return Err(LocalizedError::new("error.read_range").arg("maxLines", MAX_READ_LINES));
    }
    let content = std::fs::read_to_string(path).map_err(|e| {
        LocalizedError::new("error.read_file")
            .arg("path", path.display())
            .arg("detail", e)
    })?;
    let selected: Vec<&str> = content
        .lines()
        .skip(start_line - 1)
        .take(end_line - start_line + 1)
        .collect();
    let actual_end = start_line.saturating_add(selected.len()).saturating_sub(1);
    Ok(ReadResult {
        path: path.display().to_string(),
        start_line,
        end_line: actual_end,
        content: selected.join("\n"),
    })
}

pub fn search_for_agent(
    state: &AppState,
    root: &str,
    query: &str,
    path: Option<&str>,
) -> KfResult<SearchResult> {
    let root = project_root(state, Some(root))?;
    search_index(state, &root, query, path)
}

fn search_index(
    state: &AppState,
    root: &Path,
    query: &str,
    path: Option<&str>,
) -> KfResult<SearchResult> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Err(LocalizedError::new("error.search_query"));
    }
    let scope = indexed_search_scope(root, path)?;
    let projects = state.projects.read();
    let project = projects
        .get(root)
        .ok_or_else(|| LocalizedError::new("error.project_not_indexed"))?;
    if let Some(scope) = &scope
        && !project
            .files
            .iter()
            .any(|file| indexed_path_is_in_scope(&file.relative, scope))
    {
        return Err(LocalizedError::new("error.path_missing").arg("path", scope));
    }
    search_indexed_project(project, &needle, scope.as_deref())
}

fn search_indexed_project(
    project: &IndexedProject,
    needle: &str,
    scope: Option<&str>,
) -> KfResult<SearchResult> {
    let mut matches = Vec::new();
    let mut total = 0usize;
    for file in &project.files {
        if scope.is_some_and(|scope| !indexed_path_is_in_scope(&file.relative, scope)) {
            continue;
        }
        for line in &file.search_lines {
            if !line.folded.contains(needle) {
                continue;
            }
            total += 1;
            if matches.len() < 120 {
                matches.push(SearchMatch {
                    path: file.relative.clone(),
                    line: line.number,
                    text: line.text.trim().chars().take(240).collect(),
                });
            }
        }
    }
    Ok(SearchResult {
        truncated: total > matches.len(),
        matches,
        total,
    })
}

pub fn search_indexed(project: &IndexedProject, query: &str) -> KfResult<SearchResult> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Err(LocalizedError::new("error.search_query"));
    }
    search_indexed_project(project, &needle, None)
}

fn indexed_search_scope(root: &Path, path: Option<&str>) -> KfResult<Option<String>> {
    let Some(path) = path.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let input = Path::new(path);
    let relative = if input.is_absolute() {
        input
            .strip_prefix(root)
            .map_err(|_| LocalizedError::new("error.path_missing").arg("path", path))?
    } else {
        input
    };
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => parts.push(value.to_string_lossy().into_owned()),
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(LocalizedError::new("error.path_missing").arg("path", path));
                }
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(LocalizedError::new("error.path_missing").arg("path", path));
            }
        }
    }
    let scope = parts.join("/").to_ascii_lowercase();
    Ok((!scope.is_empty()).then_some(scope))
}

fn indexed_path_is_in_scope(path: &str, scope: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path == scope
        || path
            .strip_prefix(scope)
            .is_some_and(|tail| tail.starts_with('/'))
}

pub fn edit_exact(path: &Path, old_text: &str, new_text: &str) -> KfResult<EditResult> {
    if old_text.is_empty() {
        return Err(LocalizedError::new("error.edit_empty_match"));
    }
    let content = std::fs::read_to_string(path).map_err(|e| {
        LocalizedError::new("error.read_file")
            .arg("path", path.display())
            .arg("detail", e)
    })?;
    let count = content.match_indices(old_text).count();
    if count == 0 {
        return Err(LocalizedError::new("error.edit_not_found"));
    }
    if count != 1 {
        return Err(LocalizedError::new("error.edit_not_unique").arg("matches", count));
    }
    let next = content.replacen(old_text, new_text, 1);
    let temporary = path.with_extension(format!(
        "{}.kf-tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("file")
    ));
    std::fs::write(&temporary, next.as_bytes())
        .map_err(|e| LocalizedError::new("error.edit_write").arg("detail", e))?;
    std::fs::rename(&temporary, path).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        LocalizedError::new("error.edit_commit").arg("detail", e)
    })?;
    Ok(EditResult {
        path: path.display().to_string(),
        replacements: 1,
        bytes_written: next.len(),
    })
}

#[tauri::command]
pub fn kf_tool_read(
    state: tauri::State<'_, Arc<AppState>>,
    root: Option<String>,
    path: String,
    start_line: usize,
    end_line: usize,
) -> KfResult<ReadResult> {
    let root = project_root(&state, root.as_deref())?;
    let path = resolve_inside(&root, Path::new(&path), true)?;
    read_range(&path, start_line, end_line)
}

#[tauri::command]
pub async fn kf_tool_edit(
    state: tauri::State<'_, Arc<AppState>>,
    root: Option<String>,
    path: String,
    old_text: String,
    new_text: String,
) -> KfResult<EditResult> {
    let root = project_root(&state, root.as_deref())?;
    let path = resolve_inside(&root, Path::new(&path), true)?;
    let result = edit_exact(&path, &old_text, &new_text)?;
    refresh_indexed_file(&state, &root, &path);
    Ok(result)
}

#[tauri::command]
pub fn kf_tool_search(
    state: tauri::State<'_, Arc<AppState>>,
    root: Option<String>,
    query: String,
    path: Option<String>,
) -> KfResult<SearchResult> {
    let root = project_root(&state, root.as_deref())?;
    search_index(&state, &root, &query, path.as_deref())
}

#[tauri::command]
pub async fn kf_tool_run(
    state: tauri::State<'_, Arc<AppState>>,
    root: Option<String>,
    program: String,
    args: Vec<String>,
) -> KfResult<RunResult> {
    let root = project_root(&state, root.as_deref())?;
    run_in_root(root, program, args, &CancellationToken::new()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_edit_rejects_multiple_matches() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.txt");
        std::fs::write(&path, "same\nsame\n").unwrap();
        let error = edit_exact(&path, "same", "new").unwrap_err();
        assert_eq!(error.key, "error.edit_not_unique");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "same\nsame\n");
    }
    #[test]
    fn exact_edit_changes_one_fragment_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.txt");
        std::fs::write(&path, "before unique after").unwrap();
        edit_exact(&path, "unique", "precise").unwrap();
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "before precise after"
        );
    }
    #[test]
    fn transaction_guard_blocks_real_order_commands() {
        let arguments = vec!["order".into(), "buy".into(), "BTC".into()];
        let error = reject_real_transaction("binance", &arguments).unwrap_err();
        assert_eq!(error.key, "error.transaction_forbidden");
        assert!(reject_real_transaction("cargo", &["test".into()]).is_ok());
    }

    #[test]
    fn indexed_search_survives_source_deletion_and_returns_short_locations() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("sample.txt"),
            format!("before\nimportant needle {}\nafter\n", "value ".repeat(80)),
        )
        .unwrap();
        let indexed = crate::project::build_manifest(directory.path()).unwrap();
        let root = canonical_root(directory.path()).unwrap();
        let state = AppState::new(Default::default());
        state.projects.write().insert(root.clone(), indexed);
        std::fs::remove_file(directory.path().join("sample.txt")).unwrap();

        let result =
            search_for_agent(&state, root.to_str().unwrap(), "needle", Some("sample.txt")).unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.matches[0].path, "sample.txt");
        assert_eq!(result.matches[0].line, 2);
        assert!(result.matches[0].text.starts_with("important needle value"));
        assert!(result.matches[0].text.chars().count() <= 240);
    }

    #[test]
    fn agent_edit_refreshes_the_warm_search_index() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("sample.txt"), "old indexed value\n").unwrap();
        let indexed = crate::project::build_manifest(directory.path()).unwrap();
        let root = canonical_root(directory.path()).unwrap();
        let state = AppState::new(Default::default());
        state.projects.write().insert(root.clone(), indexed);

        edit_for_agent(
            &state,
            root.to_str().unwrap(),
            "sample.txt",
            "old indexed",
            "fresh indexed",
        )
        .unwrap();

        assert_eq!(
            search_for_agent(&state, root.to_str().unwrap(), "old indexed", None)
                .unwrap()
                .total,
            0
        );
        let refreshed =
            search_for_agent(&state, root.to_str().unwrap(), "fresh indexed", None).unwrap();
        assert_eq!(refreshed.total, 1);
        assert_eq!(refreshed.matches[0].line, 1);
    }

    #[test]
    fn agent_write_creates_and_replaces_whole_files_and_refreshes_the_index() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("sample.py"), "broken = True\n").unwrap();
        let indexed = crate::project::build_manifest(directory.path()).unwrap();
        let root = canonical_root(directory.path()).unwrap();
        let state = AppState::new(Default::default());
        state.projects.write().insert(root.clone(), indexed);

        // Replace an existing file entirely.
        let bytes = write_for_agent(
            &state,
            root.to_str().unwrap(),
            "sample.py",
            "fixed = False\n",
        )
        .unwrap();
        assert_eq!(bytes, "fixed = False\n".len() as u64);
        assert_eq!(
            std::fs::read_to_string(directory.path().join("sample.py")).unwrap(),
            "fixed = False\n"
        );
        // The warm search index sees the new content, not the old.
        assert_eq!(
            search_for_agent(&state, root.to_str().unwrap(), "broken", None)
                .unwrap()
                .total,
            0
        );

        // Create a new file in a not-yet-existing subdirectory.
        write_for_agent(
            &state,
            root.to_str().unwrap(),
            "src/new_module.py",
            "VALUE = 1\n",
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(directory.path().join("src/new_module.py")).unwrap(),
            "VALUE = 1\n"
        );
    }

    #[test]
    fn combined_command_is_normalized_without_a_shell() {
        let directory = tempfile::tempdir().unwrap();
        let (program, args) = normalize_invocation(
            directory.path(),
            "cargo test --workspace \"package name\"".into(),
            vec!["--quiet".into()],
        )
        .unwrap();
        assert_eq!(program, "cargo");
        assert_eq!(args, vec!["test", "--workspace", "package name", "--quiet"]);
        assert_eq!(
            normalize_invocation(directory.path(), "cargo \"test".into(), vec![])
                .unwrap_err()
                .key,
            "error.tool_argument"
        );
    }

    #[test]
    fn discovery_commands_are_advisory_not_rejected_and_real_commands_pass() {
        for (program, args) in [
            ("ls", vec![]),
            ("/usr/bin/tree", vec![]),
            ("cmd", vec!["/C".into(), "dir /B".into()]),
            ("cmd.exe", vec!["/cdir /s".into()]),
            (
                "cmd",
                vec![
                    "/d".into(),
                    "/c".into(),
                    "powershell -NoProfile -Command gci".into(),
                ],
            ),
            (
                "powershell.exe",
                vec!["-Command".into(), "Get-ChildItem -Force".into()],
            ),
            ("pwsh", vec!["-Com".into(), "dir -Force".into()]),
            (
                "powershell",
                vec!["-EncodedCommand".into(), "ZABpAHIA".into()],
            ),
            ("bash", vec!["-c".into(), "echo ok && ls -la".into()]),
            ("bash", vec!["-lc".into(), "command ls -la".into()]),
            ("sh", vec!["-c".into(), "env tree".into()]),
            ("zsh", vec!["-c".into(), "if true; then dir; fi".into()]),
            ("bash", vec!["-c".into(), "echo $(ls)".into()]),
            ("env", vec!["bash".into(), "-lc".into(), "exec ls".into()]),
            ("wsl", vec!["bash".into(), "-lc".into(), "ls".into()]),
            ("busybox", vec!["ls".into()]),
            ("rg", vec!["--files".into()]),
            ("grep", vec!["-R".into(), "needle".into(), ".".into()]),
            ("find", vec![".".into(), "-type".into(), "f".into()]),
            ("fd", vec!["main".into()]),
            (
                "powershell",
                vec![
                    "-NoProfile".into(),
                    "-Command".into(),
                    "Select-String -Path README.md -Pattern test".into(),
                ],
            ),
            (
                "python",
                vec![
                    "-c".into(),
                    "from pathlib import Path; print(list(Path('.').rglob('*')))".into(),
                ],
            ),
        ] {
            assert!(
                exploration_advisory(program, &args).is_some(),
                "expected advisory for {program} {args:?}"
            );
        }
        assert!(exploration_advisory("cargo", &["test".into()]).is_none());
        assert!(
            exploration_advisory("bash", &["-c".into(), "echo ls; mkdir dir".into()]).is_none()
        );
        assert!(
            exploration_advisory("python", &["-m".into(), "pytest".into(), "-q".into()]).is_none()
        );
        assert!(
            exploration_advisory("node", &["--test".into(), "test/example.test.js".into()])
                .is_none()
        );
        let advisory = exploration_advisory("ls", &[]).unwrap();
        assert!(advisory.contains("find/search/read/edit"));
    }

    #[tokio::test]
    async fn agent_run_executes_discovery_commands_and_marks_advisory() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::new(Default::default());
        let (program, args) = if cfg!(windows) {
            ("cmd".into(), vec!["/C".into(), "dir /B".into()])
        } else {
            ("sh".into(), vec!["-c".into(), "ls".into()])
        };
        let result = run_for_agent(
            &state,
            directory.path().to_str().unwrap(),
            program,
            args,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(result.exit_code, Some(0));
        let advisory = result.advisory.as_deref().unwrap();
        assert!(advisory.contains("find/search/read/edit"));
        let serialized = serde_json::to_value(&result).unwrap();
        assert!(serialized.get("advisory").is_some());
    }

    #[tokio::test]
    async fn long_output_retains_the_final_diagnostic() {
        let mut payload = vec![b'x'; MAX_OUTPUT_BYTES * 2];
        payload.extend_from_slice(b"\nFINAL_DIAGNOSTIC\n");
        let captured = read_limited(payload.as_slice()).await.unwrap();
        let output = String::from_utf8_lossy(&captured.bytes);
        assert!(captured.truncated);
        assert_eq!(captured.total_bytes, payload.len());
        assert!(output.contains("output omitted"));
        assert!(output.ends_with("FINAL_DIAGNOSTIC\n"));
        assert!(captured.bytes.len() <= MAX_OUTPUT_BYTES);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn run_tool_handles_batch_entrypoints_and_text_search() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("sample.txt"), "needle\n").unwrap();
        std::fs::write(
            directory.path().join("fixture.cmd"),
            "@echo off\r\necho batch-ready\r\n",
        )
        .unwrap();
        let token = CancellationToken::new();
        let batch = run_in_root(
            directory.path().to_path_buf(),
            "fixture".into(),
            vec![],
            &token,
        )
        .await
        .unwrap();
        assert_eq!(batch.exit_code, Some(0));
        assert!(batch.stdout.contains("batch-ready"));
        let search = run_in_root(
            directory.path().to_path_buf(),
            "powershell".into(),
            vec![
                "-NoProfile".into(),
                "-Command".into(),
                "Select-String -LiteralPath sample.txt -Pattern needle".into(),
            ],
            &token,
        )
        .await
        .unwrap();
        assert_eq!(search.exit_code, Some(0));
        assert!(search.stdout.contains("needle"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn command_mode_supports_shell_builtins_and_flags_enumeration() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::new(Default::default());
        let root = directory.path().to_str().unwrap();
        let token = CancellationToken::new();
        let result = run_command_for_agent(
            &state,
            root,
            "Write-Output shell-ready | Set-Content -LiteralPath piped.txt; Get-Content -LiteralPath piped.txt".into(),
            &token,
        )
        .await
        .unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("shell-ready"));
        assert!(directory.path().join("piped.txt").is_file());

        for command in ["dir", "Get-ChildItem -Force"] {
            let result = run_command_for_agent(&state, root, command.into(), &token)
                .await
                .unwrap();
            assert_eq!(result.exit_code, Some(0));
            assert!(result.advisory.is_some());
        }
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn command_mode_supports_shell_pipelines_and_flags_enumeration() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::new(Default::default());
        let root = directory.path().to_str().unwrap();
        let token = CancellationToken::new();
        let result = run_command_for_agent(
            &state,
            root,
            "printf shell-ready | tr a-z A-Z > piped.txt; cat piped.txt".into(),
            &token,
        )
        .await
        .unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("SHELL-READY"));
        let result = run_command_for_agent(&state, root, "ls".into(), &token)
            .await
            .unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert!(result.advisory.is_some());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn user_cancellation_interrupts_a_long_running_process() {
        let directory = tempfile::tempdir().unwrap();
        let token = CancellationToken::new();
        let cancel = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            cancel.cancel();
        });
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run_in_root(
                directory.path().to_path_buf(),
                "cmd".into(),
                vec!["/C".into(), "ping 127.0.0.1 -n 30 > nul".into()],
                &token,
            ),
        )
        .await
        .expect("cancelled process must settle promptly")
        .unwrap_err();
        assert_eq!(result.key, "error.session_cancelled");
    }
}
