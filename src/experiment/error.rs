#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    // #[error(transparent)]
    // Git(#[from] super::super::GitError),
}
