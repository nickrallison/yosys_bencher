use crate::store::{DatabaseRevision, Result, SqliteStore, StoreError, encode};
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

struct Entry<T> {
    revision: DatabaseRevision,
    key: String,
    value: Arc<T>,
}

/// A bounded LRU shared by all requests using one procedure adapter.
/// The lock also prevents duplicate concurrent loads; errors are not cached.
pub(super) struct Cache<T> {
    capacity: usize,
    entries: Mutex<VecDeque<Entry<T>>>,
}
impl<T> Cache<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Mutex::new(VecDeque::new()),
        }
    }
    pub fn get(
        &self,
        store: &SqliteStore,
        key: &impl Serialize,
        load: impl FnOnce() -> Result<T>,
    ) -> Result<Arc<T>> {
        let key = encode(key)?;
        let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let revision = store.database_revision()?;
        entries.retain(|entry| {
            entry.revision.database != revision.database || entry.revision == revision
        });
        let value = if let Some(index) = entries
            .iter()
            .position(|e| e.revision == revision && e.key == key)
        {
            let entry = entries.remove(index).unwrap();
            let value = entry.value.clone();
            entries.push_back(entry);
            value
        } else {
            let value = Arc::new(load()?);
            if store.database_revision()? != revision {
                return Err(StoreError::Expired);
            }
            entries.push_back(Entry {
                revision: revision.clone(),
                key,
                value: value.clone(),
            });
            while entries.len() > self.capacity {
                entries.pop_front();
            }
            value
        };
        if store.database_revision()? != revision {
            return Err(StoreError::Expired);
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
