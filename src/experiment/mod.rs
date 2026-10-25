use std::path::PathBuf;

use crate::src_manager::build::CcacheCMakeBuilder;
use crate::src_manager::git::history::RevisionSelection;
#[allow(deprecated)] // The runner still references the temporary store sketch.
use crate::store::Store;

mod benchmark;
mod context;
mod error;
mod suite;

pub use benchmark::{BenchmarkIn, BenchmarkOut, Procedure, ProcedureId};
pub use context::{ArtifactPolicy, RunContext};
pub use error::ExperimentError;
pub use suite::{CmpSuite, Suite, SuiteDefinition, Version};

#[derive(Debug, Clone, Getters)]
#[getset(get = "pub")]
pub struct Experiment<S: Suite> {
    revs: RevisionSelection,
    suite: S,
    output_dir: PathBuf,
}

impl<S: Suite> Experiment<S> {
    pub fn new(revs: RevisionSelection, suite: S, output_dir: PathBuf) -> Self {
        Self {
            revs,
            suite,
            output_dir,
        }
    }

    #[allow(deprecated)]
    pub fn run(
        _experiment: &Experiment<S>,
        _builder: &mut CcacheCMakeBuilder,
        _store: &mut impl Store,
        _options: &RunContext,
    ) -> Result<(), ExperimentError> {
        todo!("Stub out Experiment::run")
    }
}

#[cfg(test)]
mod tests;
