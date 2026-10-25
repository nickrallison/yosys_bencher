use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

use crate::{
    src_manager::git::history::{Commit, RevisionSelection},
    tools::perf::PerfProfiler,
};
use serde::{Deserialize, Serialize};

mod benchmark;
mod context;
mod error;
mod provider;
mod suite;

pub use benchmark::{BenchmarkIn, BenchmarkOut, Procedure, ProcedureId};
pub(crate) use context::check_label;
pub use context::{ArtifactPolicy, ProcessOutput, RunContext, repeat};
pub use error::ExperimentError;
pub use provider::{ToolProvider, YosysProvider};
pub use suite::{CmpSuite, Suite, SuiteDefinition, Version};

#[derive(Debug, Clone, Getters)]
#[getset(get = "pub")]
pub struct Experiment<S: Suite> {
    revs: RevisionSelection,
    suite: S,
    output_dir: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub artifacts: ArtifactPolicy,
    pub perf: Option<PerfProfiler>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AttemptOutcome<O> {
    Success(O),
    Failure(String),
    Unsupported(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Attempt<O> {
    pub revision: Commit,
    pub input_index: usize,
    pub started_at: SystemTime,
    /// Complete attempt time, including preparation and diagnostics.
    pub duration: Duration,
    pub artifacts: Option<PathBuf>,
    pub outcome: AttemptOutcome<O>,
}

#[derive(Debug, Serialize)]
pub struct RunReport<I, O> {
    pub suite: String,
    pub definition: SuiteDefinition<I>,
    pub directory: PathBuf,
    pub attempts: Vec<Attempt<O>>,
}

pub type ReportFor<S> = RunReport<
    <<S as Suite>::Procedure as Procedure>::Input,
    <<S as Suite>::Procedure as Procedure>::Output,
>;

impl<S: Suite> Experiment<S> {
    pub fn new(revs: RevisionSelection, suite: S, output_dir: PathBuf) -> Self {
        Self {
            revs,
            suite,
            output_dir,
        }
    }

    /// Generate inputs and session state afresh, then execute sequentially.
    /// Setup/build failures stop the invocation; workload failures become outcomes.
    pub fn run(
        &self,
        provider: &mut impl ToolProvider<Tool = S::Tool>,
        options: &RunOptions,
    ) -> Result<ReportFor<S>, ExperimentError> {
        check_label(self.suite.name())?;
        let definition = self.suite.definition()?;
        let session = self.suite.begin()?;
        let revisions = provider.revisions(&self.revs)?;
        fs::create_dir_all(&self.output_dir)?;
        let directory = tempfile::Builder::new()
            .prefix(&format!("{}-", self.suite.name()))
            .tempdir_in(&self.output_dir)?
            .keep()
            .canonicalize()?;
        let retain = *options.artifacts.logs()
            || *options.artifacts.scripts()
            || *options.artifacts.netlists()
            || options.perf.is_some();
        let mut attempts = Vec::new();
        for (revision_index, revision) in revisions.into_iter().enumerate() {
            let tool = provider.tool(&revision)?;
            let tools = self.suite.tools(&tool, &session)?;
            for (input_index, input) in definition.inputs().iter().enumerate() {
                let temporary = tempfile::Builder::new()
                    .prefix(&format!("attempt-{revision_index:04}-{input_index:04}-"))
                    .tempdir_in(&directory)?;
                let workspace = temporary.path().canonicalize()?;
                let _temporary = if retain {
                    let _ = temporary.keep();
                    None
                } else {
                    Some(temporary)
                };
                let mut context = RunContext::new(workspace.clone(), options.artifacts);
                if let Some(perf) = &options.perf {
                    context = context.with_perf(perf.clone());
                }
                let started_at = SystemTime::now();
                let start = Instant::now();
                let outcome = match self.suite.procedure().run(input, &tools, &mut context) {
                    Ok(output) => AttemptOutcome::Success(output),
                    Err(ExperimentError::Unsupported(reason)) => {
                        AttemptOutcome::Unsupported(reason)
                    }
                    Err(error) => AttemptOutcome::Failure(error.to_string()),
                };
                attempts.push(Attempt {
                    revision: revision.clone(),
                    input_index,
                    started_at,
                    duration: start.elapsed(),
                    artifacts: retain.then_some(workspace),
                    outcome,
                });
            }
        }
        Ok(RunReport {
            suite: self.suite.name().into(),
            definition,
            directory,
            attempts,
        })
    }
}

#[cfg(test)]
mod tests;
