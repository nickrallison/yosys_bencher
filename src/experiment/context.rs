use std::{
    path::PathBuf,
    process::{Command, Output},
    rc::Rc,
    time::Duration,
};

use serde::{Deserialize, Serialize};

use super::artifacts::ArtifactWriter;
use super::{ExperimentError, OsProcessExecutor, ProcessExecutor};
use crate::tools::perf::{PerfAvailability, PerfMeasurements, PerfProfiler};

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

#[derive(Debug, Getters)]
#[getset(get = "pub")]
pub struct RunContext {
    workspace: PathBuf,
    artifacts: ArtifactPolicy,
    perf: Option<PerfProfiler>,
    perf_counters: PerfAvailability,
    #[getset(skip)]
    executor: Rc<dyn ProcessExecutor>,
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
            perf_counters: PerfAvailability::Disabled,
            executor: Rc::new(OsProcessExecutor),
            processes: 0,
            sample_index: 0,
            diagnostics: Vec::new(),
        }
    }

    pub fn with_perf(mut self, perf: PerfProfiler) -> Self {
        self.perf = Some(perf);
        self
    }

    pub fn with_perf_counters(mut self, perf: PerfAvailability) -> Self {
        self.perf_counters = perf;
        self
    }

    pub fn with_executor(mut self, executor: impl ProcessExecutor + 'static) -> Self {
        self.executor = Rc::new(executor);
        self
    }

    pub(crate) fn artifact_writer(&self) -> ArtifactWriter {
        ArtifactWriter::new(self.workspace.clone())
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

    pub fn run_process(
        &mut self,
        label: &str,
        command: &mut Command,
    ) -> Result<ProcessOutput, ExperimentError> {
        check_label(label)?;
        let perf = match &self.perf_counters {
            PerfAvailability::Available(perf) => Some(perf),
            _ => None,
        };
        let captured = self.executor.execute(command, &self.workspace, perf)?;
        let index = self.processes;
        self.processes += 1;
        if self.artifacts.logs {
            let result = self.artifact_writer().process(index, label, &captured);
            self.record_diagnostic(label, result);
        }
        if let PerfAvailability::Unavailable(reason) = &self.perf_counters {
            self.diagnostics
                .push(format!("perf counters unavailable: {reason}"));
            self.perf_counters = PerfAvailability::Disabled;
        }
        let (result, diagnostics) = captured.decode();
        self.diagnostics.extend(diagnostics);
        result
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
