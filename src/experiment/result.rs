use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::SuiteDefinition;
use crate::src_manager::git::history::Commit;

pub use yosys_bencher_ui_types::experiment::{AttemptOutcome, AttemptRecord};

#[derive(Debug, Serialize, Deserialize)]
pub struct Attempt<O> {
    pub revision: Commit,
    pub input_index: usize,
    pub sample_index: u32,
    pub artifacts: Option<PathBuf>,
    #[serde(flatten)]
    pub record: AttemptRecord<O>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RevisionFailure {
    pub revision: Commit,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct RunReport<I, O> {
    pub run_id: crate::store::RunId,
    pub experiment_id: crate::store::ExperimentId,
    pub suite: String,
    pub definition: SuiteDefinition<I>,
    pub directory: PathBuf,
    pub attempts: Vec<Attempt<O>>,
    pub revision_failures: Vec<RevisionFailure>,
}
