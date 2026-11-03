use super::*;

#[derive(Clone, Serialize, Deserialize)]
struct IdentityOutput {
    value: f64,
    source: u32,
}
#[contracts::contract_trait]
impl BenchmarkOut for IdentityOutput {
    fn list_metrics() -> Vec<MetricDescriptor> {
        ["value", "missing"]
            .into_iter()
            .map(|name| MetricDescriptor::new(name, name, MetricType::Float, "", true))
            .collect()
    }
    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        (name == "value").then_some(MetricValue::Float(self.value))
    }
    fn observation_identity(&self, path: &str) -> Option<String> {
        Some(format!("source:{}:{path}", self.source))
    }
}
struct IdentityProcedure;
impl Procedure for IdentityProcedure {
    type Input = Input;
    type Output = IdentityOutput;
    type Tools = ();
    fn name(&self) -> &str {
        "identity-test"
    }
    fn run(
        &self,
        _: &Input,
        _: &(),
        _: &mut RunContext,
    ) -> std::result::Result<IdentityOutput, ExperimentError> {
        Ok(IdentityOutput {
            value: 1.,
            source: 7,
        })
    }
}
impl StoredProcedure for IdentityProcedure {
    const STORAGE_KEY: &'static str = "identity-test";
    const MEASUREMENT_CONTRACT: &'static str = "identity-test-v1";
}
inventory::submit! { AdapterRegistration::for_procedure::<IdentityProcedure>() }

#[test]
fn custom_identity_fallback_preserves_missing_and_future_paths() {
    let mut store = SqliteStore::in_memory().unwrap();
    let definition = SuiteDefinition::new(
        ProcedureId::new("identity-test", Version::default()),
        vec![Input(1)],
    )
    .unwrap();
    let run = store
        .register_run::<IdentityProcedure>(
            &ExperimentSpec {
                key: "identity".into(),
                name: "Identity".into(),
                description: String::new(),
                repository: "test".into(),
            },
            &definition,
            &RunPlan::new(vec![commit("a")], 1),
        )
        .unwrap();
    let work = run.work_items["a"][&run.inputs[0]];
    let attempt = store.start_attempt(work, 0).unwrap();
    store
        .finish_attempt::<IdentityProcedure>(
            attempt,
            &AttemptRecord {
                started_at: SystemTime::UNIX_EPOCH,
                duration: Duration::ZERO,
                outcome: AttemptOutcome::Success(IdentityOutput {
                    value: 1.,
                    source: 7,
                }),
                diagnostics: vec!["retained raw diagnostic".into()],
            },
            None,
        )
        .unwrap();
    store.finish_work_item(work).unwrap();
    let selection = ResultSelection::new(run.experiment);
    let frame = store
        .projected_frame::<IdentityProcedure>(&selection, &["missing".into()])
        .unwrap();
    let AttemptOutcome::Success(output) = &frame.attempts[0].record.as_ref().unwrap().outcome
    else {
        panic!()
    };
    assert_eq!(
        output.observation_identity("future.path"),
        Some("source:7:future.path".into())
    );
    let backend = Backend::new(store).unwrap();
    let result = backend
        .analyze(&AnalysisPlan::history(selection, "missing"))
        .unwrap();
    let inspection = backend
        .inspect(
            &result.snapshot,
            &result.series[0].points[0].id,
            Paging::default(),
        )
        .unwrap();
    assert_eq!(
        inspection.observations.items[0].identities,
        [format!("run:{}:source:7:missing", run.run)]
    );
    assert_eq!(
        inspection.attempts.items[0]
            .record
            .as_ref()
            .unwrap()
            .diagnostics,
        ["retained raw diagnostic"]
    );
}

fn fixture(store: &mut SqliteStore) -> RegisteredRun {
    let run = register(store, &[1, 2], &["a", "b"], 2);
    for revision in ["a", "b"] {
        for (index, input) in run.inputs.iter().enumerate() {
            let work = run.work_items[revision][input];
            for repetition in 0..2 {
                let value =
                    (index * 10 + repetition as usize + if revision == "a" { 2 } else { 4 }) as f64;
                let outcome = if index == 1 && repetition == 0 {
                    AttemptOutcome::Failure("test failure".into())
                } else {
                    AttemptOutcome::Success(Output { a: value, b: 2. })
                };
                record(store, work, repetition, outcome);
            }
            store.finish_work_item(work).unwrap();
        }
    }
    store.finish_run(run.run).unwrap();
    run
}

#[test]
fn projected_analysis_matches_legacy_for_operations_formulas_and_reducers() {
    let mut store = SqliteStore::in_memory().unwrap();
    let run = fixture(&mut store);
    let selection = ResultSelection::new(run.experiment);
    let raw = store.frame::<TestProcedure>(&selection).unwrap();
    let expected_catalog = crate::metrics::catalog(
        store
            .successful_outputs::<TestProcedure>(run.experiment)
            .unwrap()
            .iter(),
    );
    let backend = Backend::new(store).unwrap();
    assert_eq!(
        backend.describe(run.experiment).unwrap().metrics,
        expected_catalog
    );
    for metric in ["a", "b", "input_ratio", "suite_ratio"] {
        for operation in [
            Operation::History,
            Operation::Changes {
                comparison: Comparison::AdjacentSelected,
                threshold: Threshold {
                    kind: ChangeKind::Percent,
                    value: 5.,
                },
                ranking: Ranking::Percent,
            },
            Operation::Scaling {
                x: "width".into(),
                reduce_remaining: true,
            },
            Operation::Variation {
                population: Population::WithinRun,
            },
            Operation::Variation {
                population: Population::AcrossInputs,
            },
            Operation::Coverage,
        ] {
            for reduction in [
                Reduction::Mean,
                Reduction::Median,
                Reduction::Min,
                Reduction::Max,
            ] {
                let mut plan = AnalysisPlan::history(selection.clone(), metric);
                plan.operation = operation.clone();
                plan.repetitions = reduction;
                let legacy = crate::analysis::analyze_frame(&raw, &plan);
                let projected = backend.analyze(&plan);
                match (legacy, projected) {
                    (Ok(a), Ok(b)) => assert_eq!(
                        serde_json::to_value(a).unwrap(),
                        serde_json::to_value(b).unwrap(),
                        "{metric} {operation:?} {reduction:?}"
                    ),
                    (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
                    (a, b) => panic!("legacy/projected mismatch: {a:?} / {b:?}"),
                }
            }
        }
    }
}

#[test]
fn version_one_database_backfills_once_and_preserves_raw_records() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("results.sqlite");
    let mut store = SqliteStore::open(&path).unwrap();
    let run = fixture(&mut store);
    let raw: Vec<String> = store
        .connection
        .prepare("SELECT record FROM attempts ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    let generation = store.generation().unwrap();
    store.connection.execute_batch("DROP TABLE analysis_values; DROP TABLE analysis_records; DROP TABLE analysis_metrics; DROP TABLE analysis_inputs; DROP TABLE analysis_state; PRAGMA user_version=1;").unwrap();
    drop(store);
    let backend = Backend::open(&path).unwrap();
    let plan = AnalysisPlan::history(ResultSelection::new(run.experiment), "a");
    let result = backend.analyze(&plan).unwrap();
    assert_eq!(backend.store.generation().unwrap(), generation);
    let after: Vec<String> = backend
        .store
        .connection
        .prepare("SELECT record FROM attempts ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(raw, after);
    assert_eq!(
        backend
            .store
            .connection
            .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        2
    );
    assert_eq!(
        backend
            .store
            .connection
            .query_row("SELECT count(*) FROM analysis_records", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        8
    );
    drop(backend);
    assert_eq!(
        serde_json::to_value(Backend::open(&path).unwrap().analyze(&plan).unwrap()).unwrap(),
        serde_json::to_value(result).unwrap()
    );
}

#[test]
fn failed_backfill_rolls_back_and_can_be_retried() {
    let mut store = SqliteStore::in_memory().unwrap();
    let run = fixture(&mut store);
    let (attempt, original): (i64, String) = store
        .connection
        .query_row(
            "SELECT id,record FROM attempts ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    store
        .connection
        .execute(
            "DELETE FROM analysis_state WHERE experiment=?",
            [run.experiment.0],
        )
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE attempts SET record='broken JSON' WHERE id=?",
            [attempt],
        )
        .unwrap();
    assert!(
        store
            .ensure_projection::<TestProcedure>(run.experiment)
            .is_err()
    );
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM analysis_records", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        8
    );
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM analysis_state", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    store
        .connection
        .execute(
            "UPDATE attempts SET record=? WHERE id=?",
            rusqlite::params![original, attempt],
        )
        .unwrap();
    store
        .ensure_projection::<TestProcedure>(run.experiment)
        .unwrap();
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM analysis_records", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        8
    );
}
