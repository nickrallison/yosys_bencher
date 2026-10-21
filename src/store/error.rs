use crate::{src_manager::git::history::CommitId, store::ExperimentId};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("ExperimentId not found: {0}")]
    ExperimentIdNotFound(ExperimentId),
    #[error("CommitId not found: {0}")]
    CommitIdNotFound(CommitId),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}
