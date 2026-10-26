use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use super::ExperimentError;
use crate::tools::perf::{PerfMeasurements, PerfProfiler, PerfStat};

#[derive(Debug)]
pub struct ProcessOutput {
    pub output: Output,
    pub elapsed: Duration,
    pub perf: Option<PerfMeasurements>,
}

impl ProcessOutput {
    pub fn stdout(&self) -> Result<&str, ExperimentError> {
        std::str::from_utf8(&self.output.stdout)
            .map_err(|error| ExperimentError::Parse(error.to_string()))
    }
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
    perf_counters: Option<PerfStat>,
    processes: usize,
    sample_index: u32,
    diagnostics: Vec<String>,
}

impl RunContext {
    pub fn new(workspace: PathBuf, artifacts: ArtifactPolicy) -> Self {
        Self {
            workspace,
            artifacts,
            perf: None,
            perf_counters: None,
            processes: 0,
            sample_index: 0,
            diagnostics: Vec::new(),
        }
    }

    pub fn with_perf(mut self, perf: PerfProfiler) -> Self {
        self.perf = Some(perf);
        self
    }

    /// Experiment run options enable counters by default; standalone contexts opt in here.
    pub fn with_perf_counters(mut self, perf: Option<PerfStat>) -> Self {
        self.perf_counters = perf;
        self
    }

    pub fn with_sample_index(mut self, sample_index: u32) -> Self {
        self.sample_index = sample_index;
        self
    }

    pub fn record_diagnostic(&mut self, label: &str, result: Result<(), ExperimentError>) {
        if let Err(error) = result {
            self.diagnostics.push(format!("{label}: {error}"));
        }
    }

    /// Only spawning, execution, and output collection contribute to elapsed time.
    /// When enabled, wall time includes the perf stat wrapper's overhead.
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
        let report = self.perf_report()?;
        let mut wrapped = report
            .as_ref()
            .map(|(perf, file)| perf.command(command, file.path()));
        let measured = wrapped.as_mut().unwrap_or(command);
        measured.stdin(Stdio::null());
        let description = format!("{measured:?}");
        let index = self.processes;
        self.processes += 1;
        let start = Instant::now();
        let output = measured
            .output()
            .map_err(|source| ExperimentError::CommandIo {
                command: description.clone(),
                source,
            })?;
        let elapsed = start.elapsed();
        let perf = report.as_ref().and_then(|(_, file)| {
            match fs::read_to_string(file.path())
                .map_err(ExperimentError::from)
                .and_then(|text| PerfMeasurements::parse(&text))
            {
                Ok(perf) => Some(perf),
                Err(error) => {
                    self.record_diagnostic("perf counters", Err(error));
                    None
                }
            }
        });
        if self.artifacts.logs {
            let prefix = self.workspace.join(format!("process-{index:03}-{label}"));
            let result = (|| -> Result<(), ExperimentError> {
                fs::write(prefix.with_extension("stdout.log"), &output.stdout)?;
                fs::write(prefix.with_extension("stderr.log"), &output.stderr)?;
                fs::write(prefix.with_extension("command.log"), &description)?;
                if let Some((_, file)) = &report {
                    fs::copy(file.path(), prefix.with_extension("perf.json"))?;
                }
                Ok(())
            })();
            self.record_diagnostic(label, result);
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
        Ok(ProcessOutput {
            output,
            elapsed,
            perf,
        })
    }

    fn perf_report(
        &mut self,
    ) -> Result<Option<(PerfStat, tempfile::NamedTempFile)>, ExperimentError> {
        let Some(perf) = self.perf_counters.clone() else {
            return Ok(None);
        };
        if let Err(error) = perf.check() {
            self.record_diagnostic("perf counters unavailable", Err(error));
            self.perf_counters = None;
            return Ok(None);
        }
        Ok(Some((
            perf,
            tempfile::NamedTempFile::new_in(&self.workspace)?,
        )))
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
