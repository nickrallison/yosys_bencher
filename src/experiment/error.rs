#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("invalid tool output: {0}")]
    Parse(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("cannot run {command}: {source}")]
    CommandIo {
        command: String,
        source: std::io::Error,
    },
    #[error("process {command} failed ({status}): {detail}")]
    Process {
        command: String,
        status: std::process::ExitStatus,
        detail: String,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    #[error("duplicate benchmark input at index {index}")]
    DuplicateInput { index: usize },
    #[error("profiling error: {0}")]
    Profiling(String),
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Git(#[from] crate::src_manager::git::GitError),
    #[error(transparent)]
    History(#[from] crate::src_manager::git::history::HistoryError),
    #[error(transparent)]
    Build(#[from] crate::src_manager::build::BuildError),
}
