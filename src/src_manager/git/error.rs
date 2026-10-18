#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("invalid revision selection: {0}")]
    InvalidRevision(String),
    #[error("invalid Git output: {0}")]
    InvalidGitOutput(String),
    #[error("git {command} failed in {repository:?} ({status}): {stderr}")]
    Git {
        repository: std::path::PathBuf,
        command: String,
        status: std::process::ExitStatus,
        stderr: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
