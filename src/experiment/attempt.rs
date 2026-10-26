use std::{
    path::{Path, PathBuf},
    time::{Instant, SystemTime},
};

use super::{AttemptRecord, ExperimentError, Procedure, RunContext, RunOptions};

/// Holds temporary workspaces until the attempt completes; retained ones survive errors.
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

pub(super) fn run_attempt<P: Procedure>(
    procedure: &P,
    input: &P::Input,
    tools: &P::Tools,
    options: &RunOptions,
    directory: &Path,
    prefix: &str,
    sample_index: u32,
) -> Result<(AttemptRecord<P::Output>, Option<PathBuf>), ExperimentError> {
    let workspace = Workspace::new(directory, prefix, options.retains_artifacts())?;
    let mut context = RunContext::new(workspace.path.clone(), options.artifacts)
        .with_sample_index(sample_index)
        .with_perf_counters(options.perf_counters.clone());
    if let Some(perf) = &options.perf {
        context = context.with_perf(perf.clone());
    }
    let started_at = SystemTime::now();
    let start = Instant::now();
    let outcome = procedure.run(input, tools, &mut context).into();
    let record = AttemptRecord {
        started_at,
        duration: start.elapsed(),
        outcome,
        diagnostics: context.diagnostics().clone(),
    };
    Ok((record, workspace.artifacts()))
}
