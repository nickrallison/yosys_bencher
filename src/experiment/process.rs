use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

use super::{ExperimentError, ProcessOutput};
use crate::tools::perf::{PerfMeasurements, PreparedPerfStat};

#[derive(Debug)]
pub struct ProcessCapture {
    pub command: String,
    pub output: Output,
    pub elapsed: Duration,
    pub perf: Option<Vec<u8>>,
    pub diagnostics: Vec<String>,
}

pub trait ProcessExecutor: std::fmt::Debug {
    fn execute(
        &self,
        command: &mut Command,
        workspace: &Path,
        perf: Option<&PreparedPerfStat>,
    ) -> Result<ProcessCapture, ExperimentError>;
}

#[derive(Debug)]
pub struct OsProcessExecutor;

impl ProcessExecutor for OsProcessExecutor {
    fn execute(
        &self,
        command: &mut Command,
        workspace: &Path,
        perf: Option<&PreparedPerfStat>,
    ) -> Result<ProcessCapture, ExperimentError> {
        fs::create_dir_all(workspace)?;
        if command.get_current_dir().is_none() {
            command.current_dir(workspace.canonicalize()?);
        }
        let report = perf
            .map(|_| tempfile::NamedTempFile::new_in(workspace))
            .transpose()?;
        let mut wrapped = perf
            .zip(report.as_ref())
            .map(|(perf, file)| perf.command(command, file.path()));
        let measured = wrapped.as_mut().unwrap_or(command);
        measured.stdin(Stdio::null());
        let description = format!("{measured:?}");
        let start = Instant::now();
        let output = measured
            .output()
            .map_err(|source| ExperimentError::CommandIo {
                command: description.clone(),
                source,
            })?;
        let elapsed = start.elapsed();
        let mut diagnostics = Vec::new();
        let perf = report.and_then(|file| match fs::read(file.path()) {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                diagnostics.push(format!("perf counters: {error}"));
                None
            }
        });
        Ok(ProcessCapture {
            command: description,
            output,
            elapsed,
            perf,
            diagnostics,
        })
    }
}

impl ProcessCapture {
    pub fn decode(self) -> (Result<ProcessOutput, ExperimentError>, Vec<String>) {
        let mut diagnostics = self.diagnostics;
        let perf = self.perf.as_ref().and_then(|bytes| {
            match std::str::from_utf8(bytes)
                .map_err(|error| ExperimentError::Parse(error.to_string()))
                .and_then(PerfMeasurements::parse)
            {
                Ok(perf) => Some(perf),
                Err(error) => {
                    diagnostics.push(format!("perf counters: {error}"));
                    None
                }
            }
        });
        let result = if self.output.status.success() {
            Ok(ProcessOutput {
                output: self.output,
                elapsed: self.elapsed,
                perf,
            })
        } else {
            let detail = if self.output.stderr.is_empty() {
                &self.output.stdout
            } else {
                &self.output.stderr
            };
            let detail = String::from_utf8_lossy(&detail[detail.len().saturating_sub(8192)..])
                .trim()
                .to_owned();
            Err(ExperimentError::Process {
                command: self.command,
                status: self.output.status,
                detail,
                stdout: self.output.stdout,
                stderr: self.output.stderr,
            })
        };
        (result, diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        experiment::{ArtifactPolicy, RunContext},
        test_support::output,
    };
    use std::{cell::RefCell, collections::VecDeque, path::PathBuf, rc::Rc};

    #[derive(Debug)]
    struct RecordedExecutor {
        responses: RefCell<VecDeque<ProcessCapture>>,
        commands: Rc<RefCell<Vec<String>>>,
    }

    impl ProcessExecutor for RecordedExecutor {
        fn execute(
            &self,
            command: &mut Command,
            _: &Path,
            _: Option<&PreparedPerfStat>,
        ) -> Result<ProcessCapture, ExperimentError> {
            self.commands.borrow_mut().push(format!("{command:?}"));
            Ok(self.responses.borrow_mut().pop_front().unwrap())
        }
    }

    #[test]
    fn process_results_and_diagnostics_can_be_replayed_in_memory() {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let captured = |code, perf| ProcessCapture {
            command: "tool".into(),
            output: output(code, b"stdout", b"stderr"),
            elapsed: Duration::from_millis(3),
            perf,
            diagnostics: Vec::new(),
        };
        let executor = RecordedExecutor {
            responses: RefCell::new(VecDeque::from([
                captured(0, Some(b"malformed perf report".to_vec())),
                captured(7, None),
            ])),
            commands: Rc::clone(&commands),
        };
        let mut context = RunContext::new(
            PathBuf::from("unused-workspace"),
            ArtifactPolicy::new(false, false, false),
        )
        .with_executor(executor);
        let result = context
            .run_process("success", &mut Command::new("first"))
            .unwrap();
        assert_eq!(result.stdout().unwrap(), "stdout");
        assert_eq!(result.elapsed, Duration::from_millis(3));
        assert!(result.perf.is_none());
        assert_eq!(context.diagnostics().len(), 1);
        assert!(
            matches!(context.run_process("failure", &mut Command::new("second")), Err(ExperimentError::Process { status, detail, .. }) if status.code() == Some(7) && detail == "stderr")
        );
        assert_eq!(commands.borrow().len(), 2);
        assert_eq!(*context.processes(), 2);
    }
}
