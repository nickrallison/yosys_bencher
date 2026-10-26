use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

use super::{ExperimentError, SuiteDefinition};
use crate::src_manager::git::history::Commit;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttemptOutcome<O> {
    Success(O),
    Failure(String),
    Unsupported(String),
}

impl<O> From<Result<O, ExperimentError>> for AttemptOutcome<O> {
    fn from(result: Result<O, ExperimentError>) -> Self {
        match result {
            Ok(output) => Self::Success(output),
            Err(ExperimentError::Unsupported(reason)) => Self::Unsupported(reason),
            Err(error) => Self::Failure(error.to_string()),
        }
    }
}

/// One execution, shared by runtime reports and stored results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptRecord<O> {
    pub started_at: SystemTime,
    /// Complete attempt time, including preparation and diagnostics.
    pub duration: Duration,
    pub outcome: AttemptOutcome<O>,
    /// Artifact and profiling errors that do not invalidate the measurement.
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Attempt<O> {
    pub revision: Commit,
    pub input_index: usize,
    /// Zero-based repetition index, matched across revisions.
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
    pub suite: String,
    pub definition: SuiteDefinition<I>,
    pub directory: PathBuf,
    pub attempts: Vec<Attempt<O>>,
    pub revision_failures: Vec<RevisionFailure>,
}
