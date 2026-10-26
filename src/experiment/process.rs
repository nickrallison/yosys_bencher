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
        tools::perf::PerfAvailability,
    };
    use std::{cell::RefCell, collections::VecDeque};

    #[derive(Debug)]
    struct Replay(RefCell<VecDeque<ProcessCapture>>);
    impl ProcessExecutor for Replay {
        fn execute(
            &self,
            command: &mut Command,
            workspace: &Path,
            perf: Option<&PreparedPerfStat>,
        ) -> Result<ProcessCapture, ExperimentError> {
            assert_eq!(command.get_program(), "tool");
            assert_eq!(workspace, Path::new("unused"));
            assert!(perf.is_none());
            Ok(self
                .0
                .borrow_mut()
                .pop_front()
                .expect("unexpected execution"))
        }
    }

    #[test]
    fn context_preserves_process_results_and_treats_optional_instrumentation_as_diagnostics() {
        let captures = [0, 7].map(|code| ProcessCapture {
            command: "tool".into(),
            output: output(code, b"stdout", b"stderr"),
            elapsed: Duration::from_millis(3),
            perf: (code == 0).then(|| b"malformed perf report".to_vec()),
            diagnostics: vec![],
        });
        let mut context =
            RunContext::new("unused".into(), ArtifactPolicy::new(false, false, false))
                .with_executor(Replay(RefCell::new(captures.into())))
                .with_perf_counters(PerfAvailability::Unavailable("unavailable".into()));
        let result = context
            .run_process("success", &mut Command::new("tool"))
            .unwrap();
        assert_eq!(result.stdout().unwrap(), "stdout");
        assert_eq!(result.output.stderr, b"stderr");
        assert_eq!(result.elapsed, Duration::from_millis(3));
        assert!(result.perf.is_none());
        assert_eq!(context.diagnostics().len(), 2);
        assert!(context.diagnostics()[0].contains("unavailable"));
        assert!(context.diagnostics()[1].contains("perf counters"));
        let error = context
            .run_process("failure", &mut Command::new("tool"))
            .unwrap_err();
        assert!(
            matches!(error, ExperimentError::Process { status, detail, stdout, stderr, .. }
            if status.code() == Some(7) && detail == "stderr" && stdout == b"stdout" && stderr == b"stderr")
        );
        assert_eq!(*context.processes(), 2);
        assert_eq!(context.diagnostics().len(), 2);
    }
}
