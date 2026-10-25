use std::path::PathBuf;

#[derive(Debug, Clone, Getters)]
pub struct Yosys {
    #[get = "pub"]
    path: PathBuf,
}

impl Yosys {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}
