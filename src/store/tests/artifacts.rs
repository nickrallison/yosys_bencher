use super::*;

fn retain(store: &mut SqliteStore, directory: &Path) -> (AttemptId, ArtifactId) {
    let run = register(store, &[1], &["a"], 1);
    let attempt = store
        .start_attempt(run.work_items["a"][&run.inputs[0]], 0)
        .unwrap();
    store
        .finish_attempt::<TestProcedure>(
            attempt,
            &AttemptRecord {
                started_at: SystemTime::UNIX_EPOCH,
                duration: Duration::ZERO,
                outcome: AttemptOutcome::Success(Output { a: 1., b: 1. }),
                diagnostics: vec![],
            },
            Some(directory),
        )
        .unwrap();
    (attempt, store.artifacts(attempt).unwrap()[0].id)
}

#[test]
fn artifacts_survive_relocating_database_and_files() {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original");
    let files = original.join("artifacts/nested");
    std::fs::create_dir_all(&files).unwrap();
    std::fs::write(files.join("log.txt"), b"retained output").unwrap();
    let mut store = SqliteStore::open(original.join("output/results.sqlite")).unwrap();
    let (attempt, id) = retain(&mut store, &files);
    let path: String = store
        .connection
        .query_row("SELECT path FROM artifacts WHERE id=?", [id.0], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(Path::new(&path), Path::new("../artifacts/nested/log.txt"));
    assert_eq!(store.read_artifact(id).unwrap(), b"retained output");
    drop(store);
    let moved = root.path().join("relocated");
    std::fs::rename(original, &moved).unwrap();
    let store = SqliteStore::open(moved.join("output/results.sqlite")).unwrap();
    assert!(store.artifact(id).unwrap().available);
    assert!(store.artifacts(attempt).unwrap()[0].available);
    assert_eq!(store.read_artifact(id).unwrap(), b"retained output");
    std::fs::remove_file(moved.join("artifacts/nested/log.txt")).unwrap();
    assert!(!store.artifact(id).unwrap().available);
    assert!(!store.artifacts(attempt).unwrap()[0].available);
    assert!(store.read_artifact(id).is_err());
}

#[test]
fn in_memory_artifacts_remain_readable() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("log.txt"), b"output").unwrap();
    let mut store = SqliteStore::in_memory().unwrap();
    let (_, id) = retain(&mut store, root.path());
    assert_eq!(store.read_artifact(id).unwrap(), b"output");
}
