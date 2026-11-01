use std::{
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime},
};

use super::{ArtifactPolicy, AttemptRecord, ExperimentError, Procedure, RunContext, RunOptions};
use crate::tools::perf::{PerfAvailability, PerfProfiler};

pub(super) struct RunSettings {
    artifacts: ArtifactPolicy,
    profiler: Option<PerfProfiler>,
    counters: PerfAvailability,
    retain: bool,
}

impl RunSettings {
    pub fn prepare(options: &RunOptions) -> Self {
        Self {
            artifacts: options.artifacts,
            profiler: options.perf.clone(),
            counters: PerfAvailability::prepare(options.perf_counters.as_ref()),
            retain: options.retains_artifacts(),
        }
    }
}

struct Workspace {
    path: PathBuf,
    temporary: Option<tempfile::TempDir>,
}

impl Workspace {
    fn new(directory: &Path, prefix: &str, retain: bool) -> Result<Self, ExperimentError> {
        let temporary = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir_in(directory)?;
        let path = temporary.path().canonicalize()?;
        let temporary = if retain {
            let _ = temporary.keep();
            None
        } else {
            Some(temporary)
        };
        Ok(Self { path, temporary })
    }

    fn artifacts(&self) -> Option<PathBuf> {
        self.temporary.is_none().then(|| self.path.clone())
    }
}

pub(super) fn invocation_directory(name: &str, root: &Path) -> Result<PathBuf, ExperimentError> {
    fs::create_dir_all(root)?;
    Ok(tempfile::Builder::new()
        .prefix(&format!("{name}-"))
        .tempdir_in(root)?
        .keep()
        .canonicalize()?)
}

pub(super) fn run_attempt<P: Procedure>(
    procedure: &P,
    input: &P::Input,
    tools: &P::Tools,
    options: &RunSettings,
    directory: &Path,
    prefix: &str,
    sample_index: u32,
) -> Result<(AttemptRecord<P::Output>, Option<PathBuf>), ExperimentError> {
    let workspace = Workspace::new(directory, prefix, options.retain)?;
    let mut context = RunContext::new(workspace.path.clone(), options.artifacts)
        .with_sample_index(sample_index)
        .with_perf_counters(options.counters.clone());
    if let Some(perf) = &options.profiler {
        context = context.with_perf(perf.clone());
    }
    let started_at = SystemTime::now();
    let start = Instant::now();
    let outcome = match procedure.run(input, tools, &mut context) {
        Ok(value) => super::AttemptOutcome::Success(value),
        Err(ExperimentError::Unsupported(reason)) => super::AttemptOutcome::Unsupported(reason),
        Err(error) => super::AttemptOutcome::Failure(error.to_string()),
    };
    let record = AttemptRecord {
        started_at,
        duration: start.elapsed(),
        outcome,
        diagnostics: context.diagnostics().clone(),
    };
    Ok((record, workspace.artifacts()))
}
