use crate::errors::AppError;
use sha2::{Digest, Sha256};
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

#[derive(Debug, Clone)]
pub struct PythonEnvironmentAttestation {
    pub root: String,
    pub interpreter: String,
    pub fingerprint: String,
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

pub fn run_pytest_test_with_environment(
    test_file: &str,
    working_dir: &str,
    environment_root: Option<&str>,
) -> Result<ExecutionResult, AppError> {
    run_pytest_test_with_runtime_config(
        test_file,
        working_dir,
        environment_root,
        None,
        RunnerConfig::default(),
    )
}

pub fn run_pytest_test_with_attested_environment(
    test_file: &str,
    working_dir: &str,
    environment_root: Option<&str>,
    expected_fingerprint: Option<&str>,
) -> Result<ExecutionResult, AppError> {
    run_pytest_test_with_runtime_config(
        test_file,
        working_dir,
        environment_root,
        expected_fingerprint,
        RunnerConfig::default(),
    )
}

pub fn run_vitest_test(test_file: &str, working_dir: &str) -> Result<ExecutionResult, AppError> {
    run_vitest_test_with_config(test_file, working_dir, RunnerConfig::default())
}

pub fn run_unittest_test(test_file: &str, working_dir: &str) -> Result<ExecutionResult, AppError> {
    run_unittest_test_with_config(test_file, working_dir, RunnerConfig::default())
}

pub fn run_unittest_test_with_environment(
    test_file: &str,
    working_dir: &str,
    environment_root: Option<&str>,
) -> Result<ExecutionResult, AppError> {
    run_unittest_test_with_runtime_config(
        test_file,
        working_dir,
        environment_root,
        None,
        RunnerConfig::default(),
    )
}

pub fn run_unittest_test_with_attested_environment(
    test_file: &str,
    working_dir: &str,
    environment_root: Option<&str>,
    expected_fingerprint: Option<&str>,
) -> Result<ExecutionResult, AppError> {
    run_unittest_test_with_runtime_config(
        test_file,
        working_dir,
        environment_root,
        expected_fingerprint,
        RunnerConfig::default(),
    )
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
    run_pytest_test_with_runtime_config(test_file, working_dir, None, None, config)
}

fn run_pytest_test_with_runtime_config(
    test_file: &str,
    working_dir: &str,
    environment_root: Option<&str>,
    expected_fingerprint: Option<&str>,
    config: RunnerConfig,
) -> Result<ExecutionResult, AppError> {
    let paths = validate_execution_paths(test_file, working_dir)?;
    if paths.test.extension().and_then(|value| value.to_str()) != Some("py") {
        return Ok(blocked("PyTest execution requires a Python test file"));
    }
    let python = match resolve_python(&paths.root, environment_root, expected_fingerprint) {
        Ok(python) => python,
        Err(message) => return Ok(runtime_unavailable(&message)),
    };
    let Some(python) = python else {
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
    run_unittest_test_with_runtime_config(test_file, working_dir, None, None, config)
}

fn run_unittest_test_with_runtime_config(
    test_file: &str,
    working_dir: &str,
    environment_root: Option<&str>,
    expected_fingerprint: Option<&str>,
    config: RunnerConfig,
) -> Result<ExecutionResult, AppError> {
    let paths = validate_execution_paths(test_file, working_dir)?;
    if paths.test.extension().and_then(|value| value.to_str()) != Some("py") {
        return Ok(blocked("unittest execution requires a Python test file"));
    }
    let python = match resolve_python(&paths.root, environment_root, expected_fingerprint) {
        Ok(python) => python,
        Err(message) => return Ok(runtime_unavailable(&message)),
    };
    let Some(python) = python else {
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

fn resolve_python(
    project_root: &Path,
    environment_root: Option<&str>,
    expected_fingerprint: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    let Some(environment_root) = environment_root else {
        return Ok(find_allowed_python(project_root));
    };
    let attestation = attest_python_environment_path(project_root, environment_root)?;
    if expected_fingerprint.is_some_and(|expected| expected != attestation.fingerprint) {
        return Err(
            "Trusted Python runtime drifted after approval; trust it again before execution".into(),
        );
    }
    Ok(Some(PathBuf::from(attestation.interpreter)))
}

pub fn attest_python_environment(
    project_root: &str,
    environment_root: &str,
) -> Result<PythonEnvironmentAttestation, String> {
    let project_root = std::fs::canonicalize(project_root)
        .map_err(|_| "Target project is missing or unreadable".to_string())?;
    attest_python_environment_path(&project_root, environment_root)
}

fn attest_python_environment_path(
    project_root: &Path,
    environment_root: &str,
) -> Result<PythonEnvironmentAttestation, String> {
    let configured = Path::new(environment_root);
    if !configured.is_absolute() {
        return Err("Trusted Python environment must use an absolute path".into());
    }
    let metadata = std::fs::symlink_metadata(configured)
        .map_err(|_| "Trusted Python environment is missing or unreadable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(
            "Trusted Python environment root must be a real directory, not a symlink".into(),
        );
    }
    let root = std::fs::canonicalize(configured)
        .map_err(|_| "Trusted Python environment cannot be resolved".to_string())?;
    if root.starts_with(project_root) || project_root.starts_with(&root) {
        return Err("Trusted Python environment must be separate from the target project".into());
    }
    #[cfg(windows)]
    let candidates = [root.join("Scripts/python.exe")];
    #[cfg(not(windows))]
    let candidates = [root.join("bin/python3"), root.join("bin/python")];
    for candidate in candidates {
        if candidate.is_file() {
            let target = std::fs::canonicalize(&candidate)
                .map_err(|_| "Trusted Python interpreter cannot be resolved".to_string())?;
            if target.starts_with(project_root) {
                return Err("Trusted Python interpreter resolves into the target project".into());
            }
            let fingerprint = python_environment_fingerprint(&root, &candidate, &target)?;
            return Ok(PythonEnvironmentAttestation {
                root: root.to_string_lossy().into_owned(),
                interpreter: candidate.to_string_lossy().into_owned(),
                fingerprint,
            });
        }
    }
    Err("Trusted Python environment has no bin/python3 or bin/python interpreter".into())
}

fn python_environment_fingerprint(
    root: &Path,
    interpreter: &Path,
    target: &Path,
) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(b"speccompanion-python-runtime-v1\0");
    hasher.update(root.to_string_lossy().as_bytes());
    hasher.update(interpreter.to_string_lossy().as_bytes());
    hasher.update(target.to_string_lossy().as_bytes());
    let metadata = std::fs::metadata(target)
        .map_err(|_| "Python interpreter metadata is unavailable".to_string())?;
    if metadata.len() > 256 * 1024 * 1024 {
        return Err("Python interpreter exceeds the attestation limit".into());
    }
    hasher.update(metadata.len().to_le_bytes());
    if let Ok(modified) = metadata.modified().and_then(|time| {
        time.duration_since(std::time::UNIX_EPOCH)
            .map_err(std::io::Error::other)
    }) {
        hasher.update(modified.as_nanos().to_le_bytes());
    }
    hash_file_into(&mut hasher, target, 256 * 1024 * 1024)?;
    let config = root.join("pyvenv.cfg");
    if let Ok(bytes) = std::fs::read(&config) {
        if bytes.len() > 64 * 1024 {
            return Err("Python environment configuration exceeds the attestation limit".into());
        }
        hasher.update(&bytes);
    }
    let mut packages = Vec::new();
    if let Ok(lib_entries) = std::fs::read_dir(root.join("lib")) {
        for python_dir in lib_entries.flatten() {
            let site_packages = python_dir.path().join("site-packages");
            if let Ok(entries) = std::fs::read_dir(site_packages) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".dist-info") {
                        packages.push((name, entry.path()));
                    }
                }
            }
        }
    }
    packages.sort_by(|left, right| left.0.cmp(&right.0));
    if packages.len() > 2048 {
        return Err("Python environment package inventory exceeds the attestation limit".into());
    }
    for (package, path) in packages {
        hasher.update(package.as_bytes());
        hasher.update(b"\0");
        for evidence_file in ["METADATA", "RECORD"] {
            let path = path.join(evidence_file);
            if path.is_file() {
                hasher.update(evidence_file.as_bytes());
                hash_file_into(&mut hasher, &path, 2 * 1024 * 1024)?;
            }
        }
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn hash_file_into(hasher: &mut Sha256, path: &Path, limit: u64) -> Result<(), String> {
    let metadata =
        std::fs::metadata(path).map_err(|_| "Attested file is unreadable".to_string())?;
    if metadata.len() > limit {
        return Err("Attested file exceeds the fingerprint limit".into());
    }
    let bytes = std::fs::read(path).map_err(|_| "Attested file is unreadable".to_string())?;
    hasher.update(&bytes);
    Ok(())
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

    #[test]
    fn trusted_python_environment_must_be_absolute_and_external() {
        let fixture = Fixture::new();
        let project_root = fs::canonicalize(&fixture.root).expect("canonical project");
        let relative =
            resolve_python(&project_root, Some(".venv"), None).expect_err("relative path");
        assert!(relative.contains("absolute path"));

        let inside = fixture.root.join(".venv");
        fs::create_dir_all(inside.join("bin")).expect("environment tree");
        let rejected = resolve_python(&project_root, Some(inside.to_string_lossy().as_ref()), None)
            .expect_err("project-controlled environment");
        assert!(rejected.contains("separate from the target project"));
    }

    #[cfg(unix)]
    #[test]
    fn trusted_python_environment_root_cannot_be_a_symlink() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let environment =
            std::env::temp_dir().join(format!("spec-python-environment-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&environment).expect("environment");
        let link = fixture.root.with_extension("environment-link");
        symlink(&environment, &link).expect("environment link");
        let rejected = resolve_python(&fixture.root, Some(link.to_string_lossy().as_ref()), None)
            .expect_err("symlink root");
        assert!(rejected.contains("not a symlink"));
        let _ = fs::remove_file(link);
        let _ = fs::remove_dir_all(environment);
    }

    #[cfg(unix)]
    #[test]
    fn trusted_external_environment_executes_without_package_installation() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let environment =
            std::env::temp_dir().join(format!("spec-python-environment-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(environment.join("bin")).expect("environment bin");
        let system_python = find_allowed_python(&fixture.root).expect("system Python");
        symlink(system_python, environment.join("bin/python3")).expect("python entry");
        let test = fixture.root.join("trusted_runtime_test.py");
        fs::write(
            &test,
            "import unittest\n\nclass TrustedRuntimeTest(unittest.TestCase):\n    def test_runtime(self):\n        self.assertEqual(2 + 3, 5)\n",
        )
        .expect("test");
        let result = run_unittest_test_with_environment(
            test.to_string_lossy().as_ref(),
            fixture.root.to_string_lossy().as_ref(),
            Some(environment.to_string_lossy().as_ref()),
        )
        .expect("execution result");
        assert_eq!(result.status, "passed", "stderr: {}", result.stderr);
        let _ = fs::remove_dir_all(environment);
    }

    #[cfg(unix)]
    #[test]
    fn changed_package_inventory_expires_runtime_trust() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let environment =
            std::env::temp_dir().join(format!("spec-python-attestation-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(environment.join("bin")).expect("environment bin");
        fs::create_dir_all(environment.join("lib/python3.12/site-packages")).expect("packages");
        let system_python = find_allowed_python(&fixture.root).expect("system Python");
        symlink(system_python, environment.join("bin/python3")).expect("python entry");
        fs::write(
            environment.join("pyvenv.cfg"),
            "include-system-site-packages = false\n",
        )
        .expect("venv config");
        let dependency = environment.join("lib/python3.12/site-packages/dependency-1.0.dist-info");
        fs::create_dir_all(&dependency).expect("dependency marker");
        fs::write(
            dependency.join("RECORD"),
            "dependency.py,sha256=before,10\n",
        )
        .expect("package record");
        let project = fs::canonicalize(&fixture.root).expect("project");
        let trusted =
            attest_python_environment_path(&project, environment.to_string_lossy().as_ref())
                .expect("initial attestation");

        fs::write(dependency.join("RECORD"), "dependency.py,sha256=after,10\n")
            .expect("changed package record");
        let drift = resolve_python(
            &project,
            Some(environment.to_string_lossy().as_ref()),
            Some(&trusted.fingerprint),
        )
        .expect_err("changed inventory must expire trust");
        assert!(drift.contains("drifted after approval"));
        let _ = fs::remove_dir_all(environment);
    }

    #[test]
    #[ignore = "set SPECCOMPANION_DOGFOOD_PROJECT, SPECCOMPANION_DOGFOOD_TEST, and SPECCOMPANION_DOGFOOD_PYTHON_ENV"]
    fn dogfoods_a_dependencyful_external_python_environment() {
        let project = std::env::var("SPECCOMPANION_DOGFOOD_PROJECT").expect("dogfood project");
        let test = std::env::var("SPECCOMPANION_DOGFOOD_TEST").expect("dogfood test");
        let environment =
            std::env::var("SPECCOMPANION_DOGFOOD_PYTHON_ENV").expect("dogfood environment");
        let result = run_pytest_test_with_environment(&test, &project, Some(&environment))
            .expect("dogfood execution");
        assert_eq!(result.status, "passed", "stderr: {}", result.stderr);
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
