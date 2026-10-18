#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Git worktree has uncommitted or untracked files: {0:?}")]
    DirtyWorktree(std::path::PathBuf),
    #[error("expected the repository root at {expected:?}, found {actual:?}")]
    RepositoryRoot {
        expected: std::path::PathBuf,
        actual: std::path::PathBuf,
    },
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
