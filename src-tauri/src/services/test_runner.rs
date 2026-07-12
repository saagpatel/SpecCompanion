use crate::errors::AppError;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_OUTPUT_LIMIT: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub status: String,
    pub execution_time_ms: i64,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Copy)]
pub struct RunnerConfig {
    pub timeout: Duration,
    pub output_limit: usize,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            output_limit: DEFAULT_OUTPUT_LIMIT,
        }
    }
}

pub fn run_jest_test(test_file: &str, working_dir: &str) -> Result<ExecutionResult, AppError> {
    run_jest_test_with_config(test_file, working_dir, RunnerConfig::default())
}

pub fn run_pytest_test(test_file: &str, working_dir: &str) -> Result<ExecutionResult, AppError> {
    run_pytest_test_with_config(test_file, working_dir, RunnerConfig::default())
}

pub fn run_vitest_test(test_file: &str, working_dir: &str) -> Result<ExecutionResult, AppError> {
    run_vitest_test_with_config(test_file, working_dir, RunnerConfig::default())
}

pub fn run_unittest_test(test_file: &str, working_dir: &str) -> Result<ExecutionResult, AppError> {
    run_unittest_test_with_config(test_file, working_dir, RunnerConfig::default())
}

pub fn run_jest_test_with_config(
    test_file: &str,
    working_dir: &str,
    config: RunnerConfig,
) -> Result<ExecutionResult, AppError> {
    let paths = validate_execution_paths(test_file, working_dir)?;
    if !matches!(
        paths.test.extension().and_then(|value| value.to_str()),
        Some("js" | "jsx" | "ts" | "tsx")
    ) {
        return Ok(blocked(
            "Jest execution requires a JavaScript or TypeScript test file",
        ));
    }
    let runner = paths.root.join("node_modules/.bin/jest");
    if !runner.exists() {
        return Ok(runtime_unavailable(
            "Local Jest was not found at node_modules/.bin/jest",
        ));
    }
    let runner = std::fs::canonicalize(&runner)?;
    if !runner.starts_with(&paths.root.join("node_modules")) {
        return Ok(blocked("The Jest executable resolves outside this project"));
    }
    let mut command = Command::new(&runner);
    command
        .args([
            "--runTestsByPath",
            paths.test.to_string_lossy().as_ref(),
            "--coverage=false",
            "--verbose",
        ])
        .env(
            "PATH",
            safe_path(&paths.root, Some(runner.parent().unwrap_or(&paths.root))),
        )
        .env("NO_COLOR", "1");
    spawn_bounded(command, &paths.root, config, "Jest")
}

pub fn run_pytest_test_with_config(
    test_file: &str,
    working_dir: &str,
    config: RunnerConfig,
) -> Result<ExecutionResult, AppError> {
    let paths = validate_execution_paths(test_file, working_dir)?;
    if paths.test.extension().and_then(|value| value.to_str()) != Some("py") {
        return Ok(blocked("PyTest execution requires a Python test file"));
    }
    let Some(python) = find_allowed_python(&paths.root) else {
        return Ok(runtime_unavailable(
            "Python 3 was not found on the allowlisted PATH",
        ));
    };
    let mut command = Command::new(&python);
    command
        .args([
            "-m",
            "pytest",
            "-v",
            "--",
            paths.test.to_string_lossy().as_ref(),
        ])
        .env("PATH", safe_path(&paths.root, None))
        .env("PYTHONNOUSERSITE", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("NO_COLOR", "1");
    spawn_bounded(command, &paths.root, config, "PyTest")
}

pub fn run_vitest_test_with_config(
    test_file: &str,
    working_dir: &str,
    config: RunnerConfig,
) -> Result<ExecutionResult, AppError> {
    let paths = validate_execution_paths(test_file, working_dir)?;
    if !matches!(
        paths.test.extension().and_then(|value| value.to_str()),
        Some("js" | "jsx" | "ts" | "tsx")
    ) {
        return Ok(blocked(
            "Vitest execution requires a JavaScript or TypeScript test file",
        ));
    }
    let runner = paths.root.join("node_modules/.bin/vitest");
    if !runner.exists() {
        return Ok(runtime_unavailable(
            "Local Vitest was not found at node_modules/.bin/vitest",
        ));
    }
    let runner = std::fs::canonicalize(&runner)?;
    if !runner.starts_with(paths.root.join("node_modules")) {
        return Ok(blocked(
            "The Vitest executable resolves outside this project",
        ));
    }
    let mut command = Command::new(&runner);
    command
        .args([
            "run",
            paths.test.to_string_lossy().as_ref(),
            "--reporter=verbose",
        ])
        .env(
            "PATH",
            safe_path(&paths.root, Some(runner.parent().unwrap_or(&paths.root))),
        )
        .env("NO_COLOR", "1")
        .env("CI", "1");
    spawn_bounded(command, &paths.root, config, "Vitest")
}

pub fn run_unittest_test_with_config(
    test_file: &str,
    working_dir: &str,
    config: RunnerConfig,
) -> Result<ExecutionResult, AppError> {
    let paths = validate_execution_paths(test_file, working_dir)?;
    if paths.test.extension().and_then(|value| value.to_str()) != Some("py") {
        return Ok(blocked("unittest execution requires a Python test file"));
    }
    let Some(python) = find_allowed_python(&paths.root) else {
        return Ok(runtime_unavailable(
            "Python 3 was not found on the allowlisted PATH",
        ));
    };
    let python_path =
        std::env::join_paths([paths.root.join("src"), paths.root.clone()]).unwrap_or_default();
    let mut command = Command::new(&python);
    command
        .args([
            "-m",
            "unittest",
            "-v",
            paths.test.to_string_lossy().as_ref(),
        ])
        .env("PATH", safe_path(&paths.root, None))
        .env("PYTHONPATH", python_path)
        .env("PYTHONNOUSERSITE", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("NO_COLOR", "1");
    spawn_bounded(command, &paths.root, config, "unittest")
}

#[derive(Debug)]
struct ValidatedPaths {
    root: PathBuf,
    test: PathBuf,
}

fn validate_execution_paths(
    test_file: &str,
    working_dir: &str,
) -> Result<ValidatedPaths, AppError> {
    let root = std::fs::canonicalize(working_dir).map_err(AppError::Io)?;
    if !root.is_dir() {
        return Err(AppError::InvalidInput(
            "Test working directory is not a directory".into(),
        ));
    }
    let test = std::fs::canonicalize(test_file).map_err(AppError::Io)?;
    if !test.is_file() {
        return Err(AppError::InvalidInput("Test path is not a file".into()));
    }
    let app_temp = std::env::temp_dir().join("spec-companion-tests");
    let temp_root = app_temp
        .exists()
        .then(|| std::fs::canonicalize(&app_temp))
        .transpose()?;
    let contained = test.starts_with(&root)
        || temp_root
            .as_ref()
            .is_some_and(|allowed| test.starts_with(allowed));
    if !contained {
        return Err(AppError::InvalidInput(
            "Test path is outside the target project and SpecCompanion temporary directory".into(),
        ));
    }
    Ok(ValidatedPaths { root, test })
}

fn spawn_bounded(
    mut command: Command,
    working_dir: &Path,
    config: RunnerConfig,
    runner_name: &str,
) -> Result<ExecutionResult, AppError> {
    let start = Instant::now();
    command
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(runtime_unavailable(&format!(
                "{runner_name} runtime is unavailable"
            )));
        }
        Err(error) => return Err(AppError::Io(error)),
    };
    Ok(wait_bounded(child, config, start))
}

fn wait_bounded(mut child: Child, config: RunnerConfig, start: Instant) -> ExecutionResult {
    let stdout = child
        .stdout
        .take()
        .map(|stream| std::thread::spawn(move || read_limited(stream, config.output_limit)));
    let stderr = child
        .stderr
        .take()
        .map(|stream| std::thread::spawn(move || read_limited(stream, config.output_limit)));

    let wait = child.wait_timeout(config.timeout);
    let mut status = match wait {
        Ok(Some(status)) => {
            if status.success() {
                "passed".to_string()
            } else {
                "failed".to_string()
            }
        }
        Ok(None) => {
            terminate_process_tree(&mut child);
            let _ = child.wait();
            "timed_out".to_string()
        }
        Err(_) => {
            terminate_process_tree(&mut child);
            let _ = child.wait();
            "error".to_string()
        }
    };
    let (stdout, stdout_truncated) = join_output(stdout);
    let (mut stderr, stderr_truncated) = join_output(stderr);
    status = normalize_runtime_failure(status, &stderr);
    if status == "timed_out" {
        if !stderr.is_empty() {
            stderr.push('\n');
        }
        stderr.push_str(&format!(
            "Test timed out after {} ms",
            config.timeout.as_millis()
        ));
    }
    if stdout_truncated || stderr_truncated {
        if !stderr.is_empty() {
            stderr.push('\n');
        }
        stderr.push_str(&format!(
            "Output truncated at {} bytes per stream",
            config.output_limit
        ));
    }
    ExecutionResult {
        status,
        execution_time_ms: start.elapsed().as_millis() as i64,
        stdout,
        stderr,
    }
}

fn normalize_runtime_failure(status: String, stderr: &str) -> String {
    if status == "failed"
        && (stderr.contains("No module named pytest")
            || stderr.contains("No module named 'pytest'"))
    {
        "runtime_unavailable".into()
    } else {
        status
    }
}

fn read_limited<R: Read>(mut stream: R, limit: usize) -> (Vec<u8>, bool) {
    let mut kept = Vec::with_capacity(limit.min(8192));
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        let count = match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        let remaining = limit.saturating_sub(kept.len());
        kept.extend_from_slice(&chunk[..count.min(remaining)]);
        if count > remaining {
            truncated = true;
        }
    }
    (kept, truncated)
}

fn join_output(handle: Option<std::thread::JoinHandle<(Vec<u8>, bool)>>) -> (String, bool) {
    let (bytes, truncated) = handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    (String::from_utf8_lossy(&bytes).into_owned(), truncated)
}

fn find_allowed_python(project_root: &Path) -> Option<PathBuf> {
    for directory in safe_path_entries(project_root) {
        for name in ["python3", "python"] {
            let candidate = directory.join(name);
            if candidate.is_file() {
                if let Ok(canonical) = std::fs::canonicalize(candidate) {
                    if !canonical.starts_with(project_root) {
                        return Some(canonical);
                    }
                }
            }
        }
    }
    None
}

fn safe_path(project_root: &Path, prepend: Option<&Path>) -> std::ffi::OsString {
    let mut entries = Vec::new();
    if let Some(prepend) = prepend {
        entries.push(prepend.to_path_buf());
    }
    entries.extend(safe_path_entries(project_root));
    std::env::join_paths(entries).unwrap_or_default()
}

fn safe_path_entries(project_root: &Path) -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|value| {
            std::env::split_paths(&value)
                .filter(|entry| entry.is_absolute() && !entry.starts_with(project_root))
                .collect()
        })
        .unwrap_or_default()
}

fn runtime_unavailable(message: &str) -> ExecutionResult {
    ExecutionResult {
        status: "runtime_unavailable".into(),
        execution_time_ms: 0,
        stdout: String::new(),
        stderr: message.into(),
    }
}

fn blocked(message: &str) -> ExecutionResult {
    ExecutionResult {
        status: "blocked".into(),
        execution_time_ms: 0,
        stdout: String::new(),
        stderr: message.into(),
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_tree(child: &mut Child) {
    // The child is placed in its own process group before spawn. A negative PID
    // targets the whole group, including descendants created by a test runner.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn terminate_process_tree(child: &mut Child) {
    let _ = child.kill();
}

#[allow(dead_code)]
pub fn check_framework_available(framework: &str, working_dir: &str) -> bool {
    let Ok(root) = std::fs::canonicalize(working_dir) else {
        return false;
    };
    match framework {
        "jest" => root.join("node_modules/.bin/jest").is_file(),
        "vitest" => root.join("node_modules/.bin/vitest").is_file(),
        "pytest" => find_allowed_python(&root).is_some(),
        "unittest" => find_allowed_python(&root).is_some(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fixture {
        root: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("spec-runner-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&root).expect("fixture");
            Self { root }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn rejects_path_traversal_and_option_injection_outside_project() {
        let fixture = Fixture::new();
        let outside =
            std::env::temp_dir().join(format!("--config=evil-{}.py", uuid::Uuid::new_v4()));
        fs::write(&outside, "assert True").expect("outside file");
        let error =
            validate_execution_paths(&outside.to_string_lossy(), &fixture.root.to_string_lossy())
                .expect_err("outside path must fail");
        assert!(error.to_string().contains("outside the target project"));
        let _ = fs::remove_file(outside);
    }

    #[test]
    fn missing_local_jest_is_explicitly_unknown() {
        let fixture = Fixture::new();
        let test = fixture.root.join("safe.test.js");
        fs::write(&test, "expect(1).toBe(1)").expect("test");
        let result = run_jest_test(&test.to_string_lossy(), &fixture.root.to_string_lossy())
            .expect("result");
        assert_eq!(result.status, "runtime_unavailable");
    }

    #[test]
    fn missing_local_vitest_is_explicitly_unknown() {
        let fixture = Fixture::new();
        let test = fixture.root.join("safe.test.ts");
        fs::write(&test, "expect(add(2, 3)).toBe(5)").expect("test");
        let result = run_vitest_test(&test.to_string_lossy(), &fixture.root.to_string_lossy())
            .expect("result");
        assert_eq!(result.status, "runtime_unavailable");
    }

    #[test]
    fn output_reader_caps_bytes_without_blocking_the_stream() {
        let input = vec![b'x'; 4096];
        let (output, truncated) = read_limited(input.as_slice(), 128);
        assert_eq!(output.len(), 128);
        assert!(truncated);
    }

    #[test]
    fn missing_pytest_module_is_runtime_unavailable_not_failed() {
        assert_eq!(
            normalize_runtime_failure("failed".into(), "/usr/bin/python3: No module named pytest"),
            "runtime_unavailable"
        );
    }

    #[cfg(unix)]
    fn write_node_runner(root: &Path, name: &str, source: &str) {
        use std::os::unix::fs::PermissionsExt;
        let directory = root.join("node_modules/.bin");
        fs::create_dir_all(&directory).expect("runner directory");
        let runner = directory.join(name);
        fs::write(&runner, source).expect("runner source");
        let mut permissions = fs::metadata(&runner)
            .expect("runner metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(runner, permissions).expect("runner permissions");
    }

    #[cfg(unix)]
    #[test]
    fn command_like_test_filename_is_passed_as_data() {
        let fixture = Fixture::new();
        let marker = fixture.root.join("should-not-exist");
        let test = fixture.root.join("safe;touch should-not-exist.test.js");
        fs::write(&test, "expect(add(2, 3)).toBe(5)").expect("test");
        write_node_runner(
            &fixture.root,
            "jest",
            "#!/usr/bin/env node\nprocess.exit(0);\n",
        );
        let result = run_jest_test(&test.to_string_lossy(), &fixture.root.to_string_lossy())
            .expect("execution result");
        assert_eq!(result.status, "passed");
        assert!(
            !marker.exists(),
            "filename must never be shell-interpolated"
        );
    }

    #[cfg(unix)]
    #[test]
    fn vitest_runner_receives_the_contained_path_without_shell_interpolation() {
        let fixture = Fixture::new();
        let marker = fixture.root.join("vitest-should-not-exist");
        let test = fixture
            .root
            .join("safe;touch vitest-should-not-exist.test.ts");
        fs::write(&test, "expect(add(2, 3)).toBe(5)").expect("test");
        write_node_runner(
            &fixture.root,
            "vitest",
            "#!/usr/bin/env node\nprocess.exit(0);\n",
        );
        let result = run_vitest_test(&test.to_string_lossy(), &fixture.root.to_string_lossy())
            .expect("execution result");
        assert_eq!(result.status, "passed");
        assert!(!marker.exists());
    }

    #[test]
    fn unittest_executes_a_contained_stdlib_test_without_bytecode_writes() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.root.join("src/example")).expect("source tree");
        fs::create_dir_all(fixture.root.join("tests")).expect("test tree");
        fs::write(fixture.root.join("src/example/__init__.py"), "").expect("package");
        fs::write(
            fixture.root.join("src/example/math.py"),
            "def add(left, right):\n    return left + right\n",
        )
        .expect("source");
        let test = fixture.root.join("tests/test_math.py");
        fs::write(
            &test,
            "import unittest\nfrom example.math import add\n\nclass MathTests(unittest.TestCase):\n    def test_add(self):\n        self.assertEqual(add(2, 3), 5)\n",
        )
        .expect("test");
        let result = run_unittest_test(&test.to_string_lossy(), &fixture.root.to_string_lossy())
            .expect("execution result");
        assert_eq!(result.status, "passed", "stderr: {}", result.stderr);
        assert!(!fixture.root.join("tests/__pycache__").exists());
        assert!(!fixture.root.join("src/example/__pycache__").exists());
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_the_runner_process_tree() {
        let fixture = Fixture::new();
        let marker = fixture.root.join("descendant-survived");
        let test = fixture.root.join("timeout.test.js");
        fs::write(&test, "expect(run()).toBe('bounded')").expect("test");
        let marker_json =
            serde_json::to_string(marker.to_string_lossy().as_ref()).expect("marker json");
        let child_script =
            format!("setTimeout(() => require('fs').writeFileSync({marker_json}, 'bad'), 500)");
        let child_json = serde_json::to_string(&child_script).expect("child json");
        let runner = format!(
            "#!/usr/bin/env node\nrequire('child_process').spawn(process.execPath, ['-e', {child_json}], {{stdio:'ignore'}});\nsetTimeout(() => {{}}, 5000);\n"
        );
        write_node_runner(&fixture.root, "jest", &runner);
        let result = run_jest_test_with_config(
            &test.to_string_lossy(),
            &fixture.root.to_string_lossy(),
            RunnerConfig {
                timeout: Duration::from_millis(100),
                output_limit: 1024,
            },
        )
        .expect("execution result");
        assert_eq!(result.status, "timed_out");
        std::thread::sleep(Duration::from_millis(700));
        assert!(
            !marker.exists(),
            "descendant should be terminated with the process group"
        );
    }
}
