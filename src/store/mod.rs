mod error;
mod read;
mod types;
mod write;
pub use error::{Result, StoreError};
use rusqlite::Connection;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub use types::*;

pub struct SqliteStore {
    pub(crate) connection: Connection,
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
        connection.busy_timeout(Duration::from_secs(5))?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > 1 {
            return Err(StoreError::Unsupported(format!(
                "database schema {version}"
            )));
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.execute_batch(include_str!("schema.sql"))?;
        Ok(Self { connection })
    }
    pub fn generation(&self) -> Result<i64> {
        Ok(self
            .connection
            .query_row("SELECT generation FROM metadata WHERE id=1", [], |r| {
                r.get(0)
            })?)
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
