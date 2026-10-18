use std::num::NonZeroUsize;
use std::path::PathBuf;

pub struct CMakeBuilder {
    repository: PathBuf,
    cache_dir: PathBuf,
    jobs: Option<NonZeroUsize>,
}
