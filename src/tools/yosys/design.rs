use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Design {
    pub files: Vec<SourceFile>,
    pub headers: Vec<SourceFile>,
    pub options: ReadOptions,
    pub top: Option<String>,
    pub parameters: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: PathBuf,
    pub source: String,
}

impl SourceFile {
    pub fn new(path: impl Into<PathBuf>, source: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            source: source.into(),
        }
    }

    pub fn from_path(root: impl AsRef<Path>, path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        Ok(Self::new(
            path,
            std::fs::read_to_string(root.as_ref().join(path))?,
        ))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadOptions {
    pub system_verilog: bool,
    pub defines: BTreeMap<String, Option<String>>,
    pub include_dirs: Vec<PathBuf>,
    pub extra_args: Vec<String>,
}
