#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    #[error("profiling error: {0}")]
    Profiling(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
