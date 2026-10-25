#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    #[error("duplicate benchmark input at index {index}")]
    DuplicateInput { index: usize },
    #[error("profiling error: {0}")]
    Profiling(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
