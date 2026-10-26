use std::{num::NonZeroU32, path::PathBuf};

use crate::{
    src_manager::git::history::RevisionSelection,
    tools::perf::{PerfProfiler, PerfStat},
};

mod artifacts;
mod attempt;
mod benchmark;
mod context;
mod error;
mod process;
mod provider;
mod reference;
mod result;
mod suite;

pub use benchmark::{BenchmarkIn, BenchmarkOut, Procedure, ProcedureId};
pub(crate) use context::check_label;
pub use context::{ArtifactPolicy, ProcessOutput, RunContext};
pub use error::ExperimentError;
pub use process::{OsProcessExecutor, ProcessCapture, ProcessExecutor};
pub use provider::{ToolProvider, YosysProvider};
pub use reference::{
    Reference, ReferenceCache, ReferenceMetadata, ReferenceObservation, ReferenceSession,
};
pub use result::{Attempt, AttemptOutcome, AttemptRecord, RevisionFailure, RunReport};
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
    pub perf_counters: Option<PerfStat>,
    pub repetitions: NonZeroU32,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            artifacts: ArtifactPolicy::default(),
            perf: None,
            perf_counters: Some(PerfStat::default()),
            repetitions: NonZeroU32::new(1).unwrap(),
        }
    }
}

impl RunOptions {
    pub fn retains_artifacts(&self) -> bool {
        *self.artifacts.logs()
            || *self.artifacts.scripts()
            || *self.artifacts.netlists()
            || self.perf.is_some()
    }
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

    pub fn run(
        &self,
        provider: &mut impl ToolProvider<Tool = S::Tool>,
        options: &RunOptions,
    ) -> Result<ReportFor<S>, ExperimentError> {
        self.run_with_previous(provider, options, None)
    }

    pub fn run_with_previous(
        &self,
        provider: &mut impl ToolProvider<Tool = S::Tool>,
        options: &RunOptions,
        previous: Option<&DefinitionFor<S>>,
    ) -> Result<ReportFor<S>, ExperimentError> {
        check_label(self.suite.name())?;
        let session = self.suite.begin()?;
        let mut definition = self.suite.definition(&session)?;
        if let Some(previous) = previous {
            let next = definition.next_version(previous);
            if definition.compare(previous) == CmpSuite::Incompatible {
                log::warn!(
                    "incompatible suite change; bumping major version from {}.{} to {}.0",
                    previous.version().major(),
                    previous.version().minor(),
                    next.major()
                );
            }
            definition = definition.with_previous(previous);
        }
        let settings = attempt::RunSettings::prepare(options);
        let revisions = provider.revisions(&self.revs)?;
        let directory = attempt::invocation_directory(self.suite.name(), &self.output_dir)?;
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
                    let (record, artifacts) = attempt::run_attempt(
                        self.suite.procedure(),
                        input,
                        &tools,
                        &settings,
                        &directory,
                        &format!("attempt-{revision_index:04}-{input_index:04}-{sample_index:04}-"),
                        sample_index,
                    )?;
                    attempts.push(Attempt {
                        revision: revision.clone(),
                        input_index,
                        sample_index,
                        artifacts,
                        record,
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
