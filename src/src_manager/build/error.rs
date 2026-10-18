#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("no CMakeLists.txt in {0:?}")]
    MissingCMakeProject(std::path::PathBuf),
    #[error("{stage} failed with {status}")]
    Command {
        stage: &'static str,
        status: std::process::ExitStatus,
    },
    #[error("could not run {stage}: {source}")]
    CommandIo {
        stage: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid ccache statistics: {0}")]
    InvalidCacheStats(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
