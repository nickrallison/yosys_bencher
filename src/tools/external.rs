//! Content identity for fixed executables used alongside the tool under test.

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExecutableId(String);

#[derive(Debug, Clone)]
pub struct ExternalTool {
    pub executable: PathBuf,
}

impl ExternalTool {
    pub fn identity(&self) -> std::io::Result<ExecutableId> {
        Ok(ExecutableId(format!(
            "{:x}",
            Sha256::digest(fs::read(super::executable(&self.executable)?)?)
        )))
    }

    pub fn resolved(&self) -> std::io::Result<Self> {
        Ok(Self {
            executable: super::executable(&self.executable)?,
        })
    }
}
