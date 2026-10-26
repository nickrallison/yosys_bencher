use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExecutableId(String);

#[derive(Debug, Clone)]
pub struct ExternalTool {
    pub executable: PathBuf,
}

impl ExecutableId {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
    }
}

impl ExternalTool {
    pub fn identity(&self) -> std::io::Result<ExecutableId> {
        Ok(ExecutableId::from_bytes(&fs::read(super::executable(
            &self.executable,
        )?)?))
    }

    pub fn resolved(&self) -> std::io::Result<Self> {
        Ok(Self {
            executable: super::executable(&self.executable)?,
        })
    }
}
