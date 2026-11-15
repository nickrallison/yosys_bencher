mod error;
mod paths;
mod projection;
mod read;
mod types;
mod write;
pub use error::{Result, StoreError};
pub(crate) use projection::AnalysisFrame;
use rusqlite::Connection;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub use types::*;

pub struct SqliteStore {
    pub(crate) connection: Connection,
    database: DatabaseId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DatabaseId {
    File(PathBuf),
    Memory(u64),
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DatabaseRevision {
    pub database: DatabaseId,
    pub generation: i64,
    file: Option<(u64, SystemTime, u64, u64)>,
}
impl SqliteStore {
    pub fn default_path() -> PathBuf {
        std::env::var_os("YOSYS_BENCH_DATABASE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("output/results.sqlite"))
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        Self::from_connection(Connection::open(path)?)
    }
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(connection: Connection) -> Result<Self> {
        static NEXT_MEMORY_DATABASE: AtomicU64 = AtomicU64::new(0);
        let database = match connection.path().filter(|path| !path.is_empty()) {
            Some(path) => DatabaseId::File(std::fs::canonicalize(path)?),
            None => DatabaseId::Memory(NEXT_MEMORY_DATABASE.fetch_add(1, Ordering::Relaxed)),
        };
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.execute_batch(include_str!("schema.sql"))?;
        Ok(Self {
            connection,
            database,
        })
    }
    fn artifact_base(&self) -> Option<&Path> {
        match &self.database {
            DatabaseId::File(path) => path.parent(),
            DatabaseId::Memory(_) => None,
        }
    }
    fn artifact_path(&self, path: impl AsRef<Path>) -> PathBuf {
        match self.artifact_base() {
            Some(base) => base.join(path),
            None => path.as_ref().to_path_buf(),
        }
    }
    pub fn generation(&self) -> Result<i64> {
        Ok(self
            .connection
            .query_row("SELECT generation FROM metadata WHERE id=1", [], |r| {
                r.get(0)
            })?)
    }
    pub(crate) fn database_revision(&self) -> Result<DatabaseRevision> {
        let file = match &self.database {
            DatabaseId::File(path) => {
                let metadata = std::fs::metadata(path)?;
                #[cfg(unix)]
                let identity = {
                    use std::os::unix::fs::MetadataExt;
                    (metadata.dev(), metadata.ino())
                };
                #[cfg(not(unix))]
                let identity = (0, 0);
                Some((metadata.len(), metadata.modified()?, identity.0, identity.1))
            }
            DatabaseId::Memory(_) => None,
        };
        Ok(DatabaseRevision {
            database: self.database.clone(),
            generation: self.generation()?,
            file,
        })
    }
}
pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
pub(crate) fn encode(value: &impl Serialize) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}
pub(crate) fn decode<T: DeserializeOwned>(value: &str) -> Result<T> {
    Ok(serde_json::from_str(value)?)
}
pub(crate) fn changed(connection: &Connection) -> Result<()> {
    connection.execute("UPDATE metadata SET generation=generation+1 WHERE id=1", [])?;
    Ok(())
}
pub(crate) fn fingerprint(value: &impl Serialize) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!("{:x}", Sha256::digest(encode(value)?.as_bytes())))
}
pub(crate) fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}

#[cfg(test)]
mod tests;
