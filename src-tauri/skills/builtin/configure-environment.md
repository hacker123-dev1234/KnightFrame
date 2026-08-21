---
name: Configure Environment
description: Detect project environment, auto-configure dependencies, run scripts with sandbox isolation
type: active
match: 
---

# Configure Environment

Detect the project runtime environment and auto-configure before executing code.

## 1. Detect the project stack

Use `read_file` on `build.gradle.kts`, `package.json`, `requirements.txt`, `Cargo.toml`,
`go.mod`, or `CMakeLists.txt` to determine the language and build system.

Use `bash` to check installed runtimes:
```
python --version   (or python3 --version)
node --version
java -version
```

Failure avoided: running code with the wrong interpreter or missing dependencies.

## 2. Install missing dependencies

If `package.json` exists but `node_modules/` is missing:
```
npm install    (or yarn, pnpm)
```

If `requirements.txt` exists but modules missing:
```
pip install -r requirements.txt
```

Ask before installing system-level packages. Prefer project-local installs.

Failure avoided: import errors halfway through execution.

## 3. Execute code

- **Python**: `python <script.py>` or `python3 <script.py>`
- **Node.js**: `node <script.js>`
- **Shell**: `bash <script.sh>` (Unix) or `cmd /c <script.bat>` (Windows)
- **Gradle/Kotlin**: `./gradlew build` then `./gradlew run`
- **Single commands**: use `bash` tool directly

Write scripts with `write_file` first, then execute. Delete temp files after.

## 4. Check output

After execution:
- Exit code 0 = success; non-zero = failure → read stderr
- If output > 32000 chars, truncated — use `read_file` on written output file
- Report the exit code, output summary, and any errors

Failure avoided: silent failures passing as success, truncated output missed.
