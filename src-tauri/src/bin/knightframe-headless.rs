use std::path::PathBuf;

const USAGE: &str = "\
Usage: knightframe-headless --project <dir> --prompt <text> [options]

Required:
  --project <dir>    Project directory; built and indexed before the user turn.
  --prompt <text>    User turn text.

Options:
  --model <id>       Model id (default: the compatibility test model).
  --endpoint <url>   Chat-compatible base URL (default: the compatibility test endpoint).
  --events <path>    Write ordered RuntimeEvent JSONL here (default: stdout).
  --result <path>    Write the final machine-readable result record here (default: stdout).
  -h, --help         Show this help and exit.

Output contract: every line is one serialized RuntimeEvent, in emission order,
with no prose or ANSI codes. The stream ends with one result record containing
ok, answer, usage, model, project, and all tool calls from canonical history.
Exit code is 0 on success, 1 on a failed run, and 2 on a usage error.
";

#[derive(Debug, Clone)]
struct CliArgs {
    project: PathBuf,
    prompt: String,
    model: Option<String>,
    endpoint: Option<String>,
    events: Option<PathBuf>,
    result: Option<PathBuf>,
}

enum ParsedArgs {
    Run(CliArgs),
    Help,
}

fn required_value(
    args: &[String],
    index: &mut usize,
    inline: Option<&str>,
    flag: &str,
) -> Result<String, String> {
    if let Some(value) = inline {
        return Ok(value.to_owned());
    }
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut project = None;
    let mut prompt = None;
    let mut model = None;
    let mut endpoint = None;
    let mut events = None;
    let mut result = None;
    let mut index = 0;
    while index < args.len() {
        let raw = &args[index];
        let (flag, inline) = match raw.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value)),
            _ => (raw.as_str(), None),
        };
        match flag {
            "-h" | "--help" => return Ok(ParsedArgs::Help),
            "--project" => {
                project = Some(PathBuf::from(required_value(
                    args, &mut index, inline, flag,
                )?))
            }
            "--prompt" => prompt = Some(required_value(args, &mut index, inline, flag)?),
            "--model" => model = Some(required_value(args, &mut index, inline, flag)?),
            "--endpoint" => endpoint = Some(required_value(args, &mut index, inline, flag)?),
            "--events" => {
                events = Some(PathBuf::from(required_value(
                    args, &mut index, inline, flag,
                )?))
            }
            "--result" => {
                result = Some(PathBuf::from(required_value(
                    args, &mut index, inline, flag,
                )?))
            }
            other => return Err(format!("unknown option: {other}")),
        }
        index += 1;
    }
    let project = project.ok_or_else(|| "--project is required".to_owned())?;
    let prompt = prompt.ok_or_else(|| "--prompt is required".to_owned())?;
    if prompt.trim().is_empty() {
        return Err("--prompt must not be empty".to_owned());
    }
    Ok(ParsedArgs::Run(CliArgs {
        project,
        prompt,
        model,
        endpoint,
        events,
        result,
    }))
}

#[tokio::main]
async fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&raw) {
        Ok(ParsedArgs::Help) => {
            print!("{USAGE}");
            return;
        }
        Ok(ParsedArgs::Run(args)) => args,
        Err(message) => {
            eprintln!("knightframe-headless: {message}");
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    let options = knightframe_lib::headless::HeadlessOptions {
        project: args.project,
        prompt: args.prompt,
        model: args.model,
        endpoint: args.endpoint,
        events: args.events,
        result: args.result,
    };
    match knightframe_lib::headless::run(options).await {
        Ok(result) if result.ok => {}
        Ok(result) => {
            let detail = result
                .error
                .as_ref()
                .map(|error| format!(": {error}"))
                .unwrap_or_default();
            eprintln!("knightframe-headless: run failed{detail}");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("knightframe-headless: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<CliArgs, String> {
        let args: Vec<String> = args.iter().map(|value| (*value).to_owned()).collect();
        match parse_args(&args) {
            Ok(ParsedArgs::Run(args)) => Ok(args),
            Ok(ParsedArgs::Help) => Err("unexpected --help".into()),
            Err(message) => Err(message),
        }
    }

    #[test]
    fn full_invocation_parses_all_options() {
        let args = parse(&[
            "--project",
            "C:/proj",
            "--prompt",
            "inspect the graph",
            "--model",
            "future-code-9-free",
            "--endpoint",
            "https://example.com/v1",
            "--events",
            "out/events.jsonl",
            "--result",
            "out/result.json",
        ])
        .unwrap();
        assert_eq!(args.project, PathBuf::from("C:/proj"));
        assert_eq!(args.prompt, "inspect the graph");
        assert_eq!(args.model.as_deref(), Some("future-code-9-free"));
        assert_eq!(args.endpoint.as_deref(), Some("https://example.com/v1"));
        assert_eq!(args.events, Some(PathBuf::from("out/events.jsonl")));
        assert_eq!(args.result, Some(PathBuf::from("out/result.json")));
    }

    #[test]
    fn equals_form_and_defaults_are_supported() {
        let args = parse(&[
            "--project=C:/proj",
            "--prompt",
            "run the tests",
            "--events=out/events.jsonl",
        ])
        .unwrap();
        assert_eq!(args.project, PathBuf::from("C:/proj"));
        assert_eq!(args.prompt, "run the tests");
        assert!(args.model.is_none());
        assert!(args.endpoint.is_none());
        assert_eq!(args.events, Some(PathBuf::from("out/events.jsonl")));
        assert!(args.result.is_none());
    }

    #[test]
    fn required_flags_missing_are_rejected() {
        assert!(
            parse(&["--prompt", "hi"])
                .unwrap_err()
                .contains("--project")
        );
        assert!(
            parse(&["--project", "C:/proj"])
                .unwrap_err()
                .contains("--prompt")
        );
        assert!(
            parse(&["--project", "C:/proj", "--prompt", "   "])
                .unwrap_err()
                .contains("empty")
        );
    }

    #[test]
    fn unknown_option_and_missing_value_are_rejected() {
        assert!(parse(&["--project", "C:/proj", "--prompt", "hi", "--nope"]).is_err());
        assert!(parse(&["--project"]).is_err());
        assert!(parse(&["--project", "C:/proj", "--prompt"]).is_err());
    }

    #[test]
    fn help_is_recognized_anywhere() {
        let args = vec!["--help".to_owned()];
        assert!(matches!(parse_args(&args), Ok(ParsedArgs::Help)));
    }
}
