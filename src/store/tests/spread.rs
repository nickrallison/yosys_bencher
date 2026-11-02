use super::*;

fn repeated_dataset() -> (Backend, RegisteredRun) {
    let mut store = SqliteStore::in_memory().unwrap();
    let run = register(&mut store, &[1, 2], &["a", "b"], 3);
    for work in run.work_items.values() {
        for (input, item) in work {
            let start = if *input == run.inputs[0] { 1.0 } else { 10.0 };
            for sample in 0..3 {
                record(
                    &mut store,
                    *item,
                    sample,
                    AttemptOutcome::Success(Output {
                        a: start + sample as f64,
                        b: 2.0,
                    }),
                );
            }
            store.finish_work_item(*item).unwrap();
        }
    }
    store.finish_run(run.run).unwrap();
    (Backend::new(store).unwrap(), run)
}

#[test]
fn history_spread_only_uses_repetitions_of_one_input() {
    let (backend, run) = repeated_dataset();
    let mut plan = AnalysisPlan::history(ResultSelection::new(run.experiment), "a");
    let result = backend.analyze(&plan).unwrap();
    let point = &result.series[0].points[0];
    assert_eq!(point.value, Some(6.5));
    assert!(point.repetition_spread.is_none());

    plan.series = vec![Dimension::Input];
    let result = backend.analyze(&plan).unwrap();
    for series in &result.series {
        let point = &series.points[0];
        let mean = if series.dimensions["input"] == serde_json::json!(run.inputs[0]) {
            2.0
        } else {
            11.0
        };
        assert_eq!(point.value, Some(mean));
        let repetitions = point.repetition_spread.as_ref().unwrap();
        assert_eq!(
            (
                repetitions.count,
                repetitions.mean,
                repetitions.min,
                repetitions.max
            ),
            (3, mean, mean - 1.0, mean + 1.0)
        );
        assert!((repetitions.standard_deviation.unwrap() - 1.0).abs() < 1e-12);
    }
}

#[test]
fn derived_and_normalized_values_do_not_invent_repetition_spread() {
    let (backend, run) = repeated_dataset();
    let mut plan = AnalysisPlan::history(ResultSelection::new(run.experiment), "input_ratio");
    let result = backend.analyze(&plan).unwrap();
    let point = &result.series[0].points[0];
    assert_eq!(point.value, Some(3.25));
    assert!(point.repetition_spread.is_none());
    plan.series = vec![Dimension::Input];
    assert!(
        backend
            .analyze(&plan)
            .unwrap()
            .series
            .iter()
            .all(|s| s.points.iter().all(|p| p.repetition_spread.is_none()))
    );

    plan.series.clear();
    plan.metric = Some("a".into());
    plan.normalization = Some(Normalization {
        revision: "a".into(),
        kind: ChangeKind::Ratio,
        runs: None,
    });
    let result = backend.analyze(&plan).unwrap();
    let point = &result.series[0].points[1];
    assert_eq!(point.value, Some(1.0));
    assert!(point.repetition_spread.is_none());
}

#[test]
fn failed_repetitions_are_not_zero_samples_and_missing_points_have_no_spread() {
    let mut store = SqliteStore::in_memory().unwrap();
    let run = register(&mut store, &[1], &["a", "missing"], 3);
    let work = run.work_items["a"][&run.inputs[0]];
    record(
        &mut store,
        work,
        0,
        AttemptOutcome::Success(Output { a: 4.0, b: 2.0 }),
    );
    record(
        &mut store,
        work,
        1,
        AttemptOutcome::Failure("failed".into()),
    );
    record(
        &mut store,
        work,
        2,
        AttemptOutcome::Unsupported("unsupported".into()),
    );
    store.finish_work_item(work).unwrap();
    let backend = Backend::new(store).unwrap();
    let plan = AnalysisPlan::history(ResultSelection::new(run.experiment), "a");
    let result = backend.analyze(&plan).unwrap();
    let spread = result.series[0].points[0]
        .repetition_spread
        .as_ref()
        .unwrap();
    assert_eq!((spread.count, spread.min, spread.max), (1, 4.0, 4.0));
    assert!(spread.standard_deviation.is_none());
    let missing = &result.series[0].points[1];
    assert!(missing.value.is_none());
    assert!(missing.repetition_spread.is_none());
}

#[test]
fn different_runs_do_not_contribute_to_the_same_repetition_bars() {
    let mut store = SqliteStore::in_memory().unwrap();
    let mut experiment = None;
    for start in [1.0, 20.0] {
        let run = register(&mut store, &[1], &["a"], 2);
        experiment = Some(run.experiment);
        let work = run.work_items["a"][&run.inputs[0]];
        for sample in 0..2 {
            record(
                &mut store,
                work,
                sample,
                AttemptOutcome::Success(Output {
                    a: start + sample as f64 * 2.0,
                    b: 1.0,
                }),
            );
        }
        store.finish_work_item(work).unwrap();
        store.finish_run(run.run).unwrap();
    }
    let backend = Backend::new(store).unwrap();
    let mut selection = ResultSelection::new(experiment.unwrap());
    selection.runs = RunSelection::All;
    let mut plan = AnalysisPlan::history(selection, "a");
    plan.runs = Some(RunTreatment::Pool);
    assert!(
        backend.analyze(&plan).unwrap().series[0].points[0]
            .repetition_spread
            .is_none()
    );
    plan.runs = Some(RunTreatment::Separate);
    let result = backend.analyze(&plan).unwrap();
    assert_eq!(result.series.len(), 2);
    for series in &result.series {
        let spread = series.points[0].repetition_spread.as_ref().unwrap();
        assert_eq!(spread.count, 2);
        assert_eq!(spread.max - spread.min, 2.0);
    }
}
