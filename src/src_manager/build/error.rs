#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("invalid Yosys build cache: {0}")]
    BuildCache(String),
    #[error("Yosys build for {commit} failed during {stage}: {detail}; log: {log:?}")]
    YosysBuild {
        commit: String,
        stage: &'static str,
        detail: String,
        log: std::path::PathBuf,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    GitError(#[from] super::super::git::GitError),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}
