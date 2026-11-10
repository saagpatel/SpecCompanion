//! Isolated, sequential cargo execution for one link.
use super::{generate, subject::Subject, Options, RecordState};
use crate::{
    adapters::parse_libtest,
    core::{
        mutation::{Mutant, MutantOutcome, TargetKind},
        test_identity::TestStatus,
    },
};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct Runner {
    _temp: tempfile::TempDir,
    copy: PathBuf,
    target: PathBuf,
}
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_name() == "target" || entry.file_name() == ".git" {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        } else if kind.is_file() {
            fs::copy(entry.path(), to.join(entry.file_name()))?;
        } else {
            return Err(io::Error::other("unsupported crate entry"));
        }
    }
    Ok(())
}
#[derive(Debug, PartialEq, Eq)]
enum Run {
    Passed,
    Failed,
    Timeout,
    Unviable,
    Infrastructure,
}
impl Runner {
    pub fn new(subject: &Subject) -> io::Result<Self> {
        if !cfg!(unix) {
            return Err(io::Error::other("process group termination unavailable"));
        }
        let temp = tempfile::tempdir()?;
        let copy = temp.path().join("crate");
        copy_tree(&subject.package_dir, &copy)?;
        let target = temp.path().join("target");
        Ok(Self {
            _temp: temp,
            copy,
            target,
        })
    }
    fn run(&self, options: &Options, subject: &Subject, id: &str) -> Run {
        let Some(name) = id.splitn(4, ':').nth(3) else {
            return Run::Infrastructure;
        };
        let mut command = Command::new(&options.cargo);
        command
            .current_dir(&self.copy)
            .args(["test", "--offline", "--locked"]);
        match subject.target.kind {
            TargetKind::Lib => {
                command.arg("--lib");
            }
            TargetKind::Test => {
                command.args(["--test", &subject.target.name]);
            }
        }
        command
            .args([name, "--", "--exact"])
            .env("CARGO_TARGET_DIR", &self.target)
            .env("CARGO_TERM_COLOR", "never")
            .env("NO_COLOR", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        command.process_group(0);
        let Ok(mut child) = command.spawn() else {
            return Run::Infrastructure;
        };
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        let out = thread::spawn(move || {
            let mut b = Vec::new();
            stdout.read_to_end(&mut b).map(|_| b)
        });
        let err = thread::spawn(move || {
            let mut b = Vec::new();
            stderr.read_to_end(&mut b).map(|_| b)
        });
        let start = Instant::now();
        let mut status = None;
        loop {
            if status.is_none() {
                match child.try_wait() {
                    Ok(value) => status = value,
                    Err(_) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Run::Infrastructure;
                    }
                }
            }
            if status.is_some() && out.is_finished() && err.is_finished() {
                break;
            }
            if start.elapsed() >= options.timeout {
                #[cfg(unix)]
                unsafe {
                    unsafe extern "C" {
                        fn kill(pid: i32, sig: i32) -> i32;
                    }
                    kill(-(child.id() as i32), 9);
                }
                let _ = child.kill();
                let _ = child.wait();
                return Run::Timeout;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let (Ok(Ok(out)), Ok(Ok(err))) = (out.join(), err.join()) else {
            return Run::Infrastructure;
        };
        let Some(status) = status else {
            return Run::Infrastructure;
        };
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&err),
            String::from_utf8_lossy(&out)
        );
        match parse_libtest(&combined) {
            Ok(cases)
                if cases.len() == 1 && (cases[0].test_id == id || cases[0].test_id == name) =>
            {
                match cases[0].status {
                    TestStatus::Passed if status.success() => Run::Passed,
                    TestStatus::Failed if !status.success() => Run::Failed,
                    _ => Run::Infrastructure,
                }
            }
            _ if !status.success()
                && (combined.contains("error[E") || combined.contains("could not compile"))
                && !combined.contains("test result:") =>
            {
                Run::Unviable
            }
            _ => Run::Infrastructure,
        }
    }
    pub(super) fn execute(
        &self,
        options: &Options,
        subject: &Subject,
        id: &str,
        record: &mut RecordState,
    ) {
        for _ in 0..2 {
            if self.run(options, subject, id) != Run::Passed {
                record.failure("baseline failed or flaky");
                return;
            }
            record.baseline_passes += 1;
        }
        for candidate in generate::candidates_for(subject, options.max_mutants) {
            let path = self.copy.join(&candidate.mutant.file);
            let result = (|| -> io::Result<Run> {
                let source = fs::read_to_string(&path)?;
                let changed = generate::apply(
                    &source,
                    candidate.span.clone(),
                    &candidate.mutant.replacement,
                )
                .ok_or_else(|| io::Error::other("invalid mutation span"))?;
                fs::write(&path, changed)?;
                let result = self.run(options, subject, id);
                fs::write(&path, source)?;
                Ok(result)
            })();
            let outcome = match result {
                Ok(Run::Passed) => MutantOutcome::Survived,
                Ok(Run::Failed) => MutantOutcome::Killed,
                Ok(Run::Timeout) => MutantOutcome::Timeout,
                Ok(Run::Unviable) => MutantOutcome::Unviable,
                _ => {
                    record.failure("mutation execution infrastructure failure");
                    return;
                }
            };
            record.mutants.push(Mutant {
                outcome,
                ..candidate.mutant
            });
        }
    }
}
