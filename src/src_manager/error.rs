#[derive(Debug, thiserror::Error)]
pub enum SourceManagerError {
    #[error(transparent)]
    Git(#[from] super::git::GitError),
}
