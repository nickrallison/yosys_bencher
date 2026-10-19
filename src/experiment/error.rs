#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    // #[error(transparent)]
    // Git(#[from] super::super::GitError),
    #[error("profiling error: {0}")]
    Profiling(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
