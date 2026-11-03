use super::*;
use crate::store::changed;
use std::sync::{
    Barrier,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn cached_values_survive_closing_and_reopening_the_only_connection() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("results.sqlite");
    drop(SqliteStore::open(&path).unwrap());
    let cache = Cache::new(2);
    let first = {
        let store = SqliteStore::open(&path).unwrap();
        cache.get(&store, &0, || Ok(42)).unwrap()
    };
    let store = SqliteStore::open(&path).unwrap();
    let hit = cache.get(&store, &0, || Ok(43)).unwrap();
    assert!(Arc::ptr_eq(&first, &hit));
}

#[test]
fn cached_values_survive_new_connections_and_invalidate_after_external_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("results.sqlite");
    let writer = SqliteStore::open(&path).unwrap();
    let cache = Cache::new(2);
    let first = cache.get(&writer, &"selection", || Ok(42)).unwrap();
    let reader = SqliteStore::open(&path).unwrap();
    let hit = cache
        .get(&reader, &"selection", || panic!("decoded twice"))
        .unwrap();
    assert!(Arc::ptr_eq(&first, &hit));
    changed(&writer.connection).unwrap();
    let fresh = cache.get(&reader, &"selection", || Ok(43)).unwrap();
    assert_eq!(*fresh, 43);
    assert!(!Arc::ptr_eq(&first, &fresh));
}

#[test]
fn databases_with_equal_generations_and_replaced_files_do_not_share_results() {
    let cache = Cache::new(3);
    let a = SqliteStore::in_memory().unwrap();
    let b = SqliteStore::open(":memory:").unwrap();
    cache.get(&a, &0, || Ok(1)).unwrap();
    assert_eq!(*cache.get(&b, &0, || Ok(2)).unwrap(), 2);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("results.sqlite");
    let replacement = directory.path().join("replacement.sqlite");
    let store = SqliteStore::open(&path).unwrap();
    cache.get(&store, &0, || Ok(3)).unwrap();
    drop(store);
    drop(SqliteStore::open(&replacement).unwrap());
    std::fs::rename(&replacement, &path).unwrap();
    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(*cache.get(&store, &0, || Ok(4)).unwrap(), 4);
}

#[test]
fn errors_and_results_crossing_a_database_change_are_not_cached() {
    let store = SqliteStore::in_memory().unwrap();
    let cache = Cache::new(2);
    assert!(
        cache
            .get(&store, &0, || -> Result<usize> {
                Err(StoreError::Invalid("test".into()))
            })
            .is_err()
    );
    assert!(matches!(
        cache.get(&store, &0, || {
            changed(&store.connection)?;
            Ok(1)
        }),
        Err(StoreError::Expired)
    ));
    assert_eq!(*cache.get(&store, &0, || Ok(2)).unwrap(), 2);
}

#[test]
fn least_recently_used_selections_are_evicted() {
    let store = SqliteStore::in_memory().unwrap();
    let cache = Cache::new(2);
    cache.get(&store, &"a", || Ok(1)).unwrap();
    cache.get(&store, &"b", || Ok(2)).unwrap();
    cache
        .get(&store, &"a", || panic!("a was evicted early"))
        .unwrap();
    cache.get(&store, &"c", || Ok(3)).unwrap();
    assert_eq!(*cache.get(&store, &"b", || Ok(4)).unwrap(), 4);
}

#[test]
fn concurrent_requests_decode_a_selection_once() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("results.sqlite");
    drop(SqliteStore::open(&path).unwrap());
    let cache = Arc::new(Cache::new(2));
    let loads = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(4));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let (path, cache, loads, barrier) =
                (path.clone(), cache.clone(), loads.clone(), barrier.clone());
            std::thread::spawn(move || {
                let store = SqliteStore::open(&path).unwrap();
                barrier.wait();
                cache
                    .get(&store, &0, || {
                        loads.fetch_add(1, Ordering::Relaxed);
                        Ok(42)
                    })
                    .unwrap()
            })
        })
        .collect();
    let values: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(loads.load(Ordering::Relaxed), 1);
    assert!(values.iter().all(|v| Arc::ptr_eq(v, &values[0])));
}
