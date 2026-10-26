use std::{
    fs,
    num::NonZeroU32,
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
pub use context::{ArtifactPolicy, ProcessOutput, RunContext};
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

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub artifacts: ArtifactPolicy,
    pub perf: Option<PerfProfiler>,
    /// Independent attempts per input and revision, outside workload identity.
    pub repetitions: NonZeroU32,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            artifacts: ArtifactPolicy::default(),
            perf: None,
            repetitions: NonZeroU32::new(1).unwrap(),
        }
    }
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
    /// Zero-based repetition index, matched across revisions.
    pub sample_index: u32,
    /// Artifact and profiling errors that do not invalidate the measurement.
    pub diagnostics: Vec<String>,
    pub started_at: SystemTime,
    /// Complete attempt time, including preparation and diagnostics.
    pub duration: Duration,
    pub artifacts: Option<PathBuf>,
    pub outcome: AttemptOutcome<O>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RevisionFailure {
    pub revision: Commit,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct RunReport<I, O> {
    pub suite: String,
    pub definition: SuiteDefinition<I>,
    pub directory: PathBuf,
    pub attempts: Vec<Attempt<O>>,
    pub revision_failures: Vec<RevisionFailure>,
}

pub type DefinitionFor<S> = SuiteDefinition<<<S as Suite>::Procedure as Procedure>::Input>;

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
    /// Revision setup failures and workload failures are retained in the report.
    pub fn run(
        &self,
        provider: &mut impl ToolProvider<Tool = S::Tool>,
        options: &RunOptions,
    ) -> Result<ReportFor<S>, ExperimentError> {
        self.run_with_previous(provider, options, None)
    }

    /// Supply the previous stored definition to resolve the shared suite/procedure version.
    pub fn run_with_previous(
        &self,
        provider: &mut impl ToolProvider<Tool = S::Tool>,
        options: &RunOptions,
        previous: Option<&DefinitionFor<S>>,
    ) -> Result<ReportFor<S>, ExperimentError> {
        check_label(self.suite.name())?;
        let mut definition = self.suite.definition()?;
        if let Some(previous) = previous {
            definition = definition.with_previous(previous);
        }
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
        let mut revision_failures = Vec::new();
        for (revision_index, revision) in revisions.into_iter().enumerate() {
            let tools = match provider
                .tool(&revision)
                .and_then(|tool| self.suite.tools(&tool, &session))
            {
                Ok(tools) => tools,
                Err(error) => {
                    revision_failures.push(RevisionFailure {
                        revision,
                        reason: error.to_string(),
                    });
                    continue;
                }
            };
            for (input_index, input) in definition.inputs().iter().enumerate() {
                for sample_index in 0..options.repetitions.get() {
                    let temporary = tempfile::Builder::new()
                        .prefix(&format!(
                            "attempt-{revision_index:04}-{input_index:04}-{sample_index:04}-"
                        ))
                        .tempdir_in(&directory)?;
                    let workspace = temporary.path().canonicalize()?;
                    let _temporary = if retain {
                        let _ = temporary.keep();
                        None
                    } else {
                        Some(temporary)
                    };
                    let mut context = RunContext::new(workspace.clone(), options.artifacts)
                        .with_sample_index(sample_index);
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
                        sample_index,
                        diagnostics: context.diagnostics().clone(),
                        started_at,
                        duration: start.elapsed(),
                        artifacts: retain.then_some(workspace),
                        outcome,
                    });
                }
            }
        }
        Ok(RunReport {
            suite: self.suite.name().into(),
            definition,
            directory,
            attempts,
            revision_failures,
        })
    }
}

#[cfg(test)]
mod tests;
