#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Build(#[from] crate::src_manager::build::BuildError),
    #[error(transparent)]
    Git(#[from] crate::src_manager::git::GitError),
    #[error(transparent)]
    History(#[from] crate::src_manager::git::history::HistoryError),
}
