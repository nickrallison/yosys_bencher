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
mod input;
mod process;
mod provider;
mod reference;
mod result;
mod suite;

pub use benchmark::{BenchmarkIn, BenchmarkOut, Procedure, ProcedureId};
pub(crate) use context::check_label;
pub use context::{ArtifactPolicy, ProcessOutput, RunContext};
pub use error::ExperimentError;
pub use input::{FieldKind, Filter, FilterOperator, InputDescription, InputField, Page};
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
}

impl<S: Suite> Experiment<S>
where
    S::Procedure: crate::store::StoredProcedure,
{
    pub fn run(
        &self,
        provider: &mut impl ToolProvider<Tool = S::Tool>,
        options: &RunOptions,
    ) -> Result<ReportFor<S>, ExperimentError> {
        let mut store = crate::store::SqliteStore::open(crate::store::SqliteStore::default_path())?;
        let experiment = crate::store::ExperimentSpec {
            key: self.suite.name().into(),
            name: self.suite.name().into(),
            description: String::new(),
            repository: provider.repository(),
        };
        self.run_stored(&mut store, &experiment, provider, options)
    }

    /// Stored definition lineage is authoritative; callers no longer supply a previous definition.
    #[deprecated(note = "Use run or run_stored; version history is resolved in SQLite")]
    pub fn run_with_previous(
        &self,
        provider: &mut impl ToolProvider<Tool = S::Tool>,
        options: &RunOptions,
        _previous: Option<&DefinitionFor<S>>,
    ) -> Result<ReportFor<S>, ExperimentError> {
        self.run(provider, options)
    }

    pub fn run_stored(
        &self,
        store: &mut crate::store::SqliteStore,
        experiment: &crate::store::ExperimentSpec,
        provider: &mut impl ToolProvider<Tool = S::Tool>,
        options: &RunOptions,
    ) -> Result<ReportFor<S>, ExperimentError> {
        check_label(self.suite.name())?;
        let session = self.suite.begin()?;
        let definition = self.suite.definition(&session)?;
        let settings = attempt::RunSettings::prepare(options);
        let revisions = provider.revisions(&self.revs)?;
        let directory = attempt::invocation_directory(self.suite.name(), &self.output_dir)?;
        let mut plan = crate::store::RunPlan::new(revisions.clone(), options.repetitions.get());
        plan.history = provider.history();
        plan.options = serde_json::json!({ "tool": provider.measurement_options(),
            "perf_counters": options.perf_counters.is_some(), "runner": env!("CARGO_PKG_VERSION") });
        let registered = store.register_run::<S::Procedure>(experiment, &definition, &plan)?;
        let definition = definition.with_version(registered.definition.version);
        let mut attempts = Vec::new();
        let mut revision_failures = Vec::new();
        let result = (|| -> Result<(), ExperimentError> {
            for (revision_index, revision) in revisions.into_iter().enumerate() {
                let tools = match provider
                    .tool(&revision)
                    .and_then(|tool| self.suite.tools(&tool, &session))
                {
                    Ok(tools) => tools,
                    Err(error) => {
                        store.fail_revision(
                            registered.run,
                            revision.id().id(),
                            &error.to_string(),
                        )?;
                        revision_failures.push(RevisionFailure {
                            revision,
                            reason: error.to_string(),
                        });
                        continue;
                    }
                };
                for (input_index, input) in definition.inputs().iter().enumerate() {
                    let work =
                        registered.work_items[revision.id().id()][&registered.inputs[input_index]];
                    for sample_index in 0..options.repetitions.get() {
                        let id = store.start_attempt(work, sample_index)?;
                        let (record, artifacts) = attempt::run_attempt(
                            self.suite.procedure(),
                            input,
                            &tools,
                            &settings,
                            &directory,
                            &format!(
                                "attempt-{revision_index:04}-{input_index:04}-{sample_index:04}-"
                            ),
                            sample_index,
                        )
                        .unwrap_or_else(|error| {
                            (
                                AttemptRecord {
                                    started_at: std::time::SystemTime::now(),
                                    duration: std::time::Duration::ZERO,
                                    outcome: AttemptOutcome::Failure(error.to_string()),
                                    diagnostics: vec![],
                                },
                                None,
                            )
                        });
                        store.finish_attempt::<S::Procedure>(id, &record, artifacts.as_deref())?;
                        attempts.push(Attempt {
                            revision: revision.clone(),
                            input_index,
                            sample_index,
                            artifacts,
                            record,
                        });
                    }
                    store.finish_work_item(work)?;
                }
            }
            store.finish_run(registered.run)?;
            Ok(())
        })();
        if let Err(error) = result {
            if let Err(interrupt) = store.interrupt_run(registered.run, &error.to_string()) {
                log::warn!("Cannot mark interrupted run: {interrupt}");
            }
            return Err(error);
        }
        Ok(RunReport {
            run_id: registered.run,
            experiment_id: registered.experiment,
            suite: self.suite.name().into(),
            definition,
            directory,
            attempts,
            revision_failures,
        })
    }
}
