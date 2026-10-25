use std::{
    fs,
    num::NonZeroU32,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use super::ExperimentError;
use crate::tools::perf::PerfProfiler;

#[derive(Debug)]
pub struct ProcessOutput {
    pub output: Output,
    pub elapsed: Duration,
}

impl ProcessOutput {
    pub fn stdout(&self) -> Result<&str, ExperimentError> {
        std::str::from_utf8(&self.output.stdout)
            .map_err(|error| ExperimentError::Parse(error.to_string()))
    }
}

pub fn repeat<T>(
    count: NonZeroU32,
    mut run: impl FnMut(u32) -> Result<T, ExperimentError>,
) -> Result<Vec<T>, ExperimentError> {
    (0..count.get()).map(&mut run).collect()
}

pub(crate) fn check_label(label: &str) -> Result<(), ExperimentError> {
    if label.is_empty()
        || !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ExperimentError::Config(format!(
            "invalid artifact label {label:?}"
        )));
    }
    Ok(())
}

/// Runtime resources for one attempt, outside workload identity.
#[derive(Debug, Getters)]
#[getset(get = "pub")]
pub struct RunContext {
    workspace: PathBuf,
    artifacts: ArtifactPolicy,
    perf: Option<PerfProfiler>,
    processes: usize,
}

impl RunContext {
    pub fn new(workspace: PathBuf, artifacts: ArtifactPolicy) -> Self {
        Self {
            workspace,
            artifacts,
            perf: None,
            processes: 0,
        }
    }

    pub fn with_perf(mut self, perf: PerfProfiler) -> Self {
        self.perf = Some(perf);
        self
    }

    /// Only spawning, execution, and output collection contribute to elapsed time.
    /// Logs are retained here; callers handle scripts, netlists, and perf diagnostics.
    pub fn run_process(
        &mut self,
        label: &str,
        command: &mut Command,
    ) -> Result<ProcessOutput, ExperimentError> {
        check_label(label)?;
        fs::create_dir_all(&self.workspace)?;
        if command.get_current_dir().is_none() {
            command.current_dir(self.workspace.canonicalize()?);
        }
        command.stdin(Stdio::null());
        let description = format!("{command:?}");
        let index = self.processes;
        self.processes += 1;
        let start = Instant::now();
        let output = command
            .output()
            .map_err(|source| ExperimentError::CommandIo {
                command: description.clone(),
                source,
            })?;
        let elapsed = start.elapsed();
        if self.artifacts.logs {
            let prefix = self.workspace.join(format!("process-{index:03}-{label}"));
            fs::write(prefix.with_extension("stdout.log"), &output.stdout)?;
            fs::write(prefix.with_extension("stderr.log"), &output.stderr)?;
            fs::write(prefix.with_extension("command.log"), &description)?;
        }
        if !output.status.success() {
            let detail = if output.stderr.is_empty() {
                &output.stdout
            } else {
                &output.stderr
            };
            let detail = String::from_utf8_lossy(&detail[detail.len().saturating_sub(8192)..])
                .trim()
                .to_owned();
            return Err(ExperimentError::Process {
                command: description,
                status: output.status,
                detail,
                stdout: output.stdout,
                stderr: output.stderr,
            });
        }
        Ok(ProcessOutput { output, elapsed })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct ArtifactPolicy {
    logs: bool,
    scripts: bool,
    netlists: bool,
}

impl ArtifactPolicy {
    pub fn new(logs: bool, scripts: bool, netlists: bool) -> Self {
        Self {
            logs,
            scripts,
            netlists,
        }
    }
}

impl Default for ArtifactPolicy {
    fn default() -> Self {
        Self::new(true, true, false)
    }
}
