use super::*;

#[test]
fn cached_analysis_distinguishes_metrics_inputs_commits_and_runs() {
    let mut store = SqliteStore::in_memory().unwrap();
    let old = register(&mut store, &[1, 2], &["a", "b"], 1);
    fill(&mut store, &old, "a", 0, 1., 10.);
    fill(&mut store, &old, "a", 1, 3., 30.);
    fill(&mut store, &old, "b", 0, 5., 50.);
    fill(&mut store, &old, "b", 1, 7., 70.);
    let new = register(&mut store, &[1, 2], &["a", "b"], 1);
    fill(&mut store, &new, "a", 0, 11., 110.);
    fill(&mut store, &new, "a", 1, 13., 130.);
    fill(&mut store, &new, "b", 0, 15., 150.);
    fill(&mut store, &new, "b", 1, 17., 170.);
    let backend = Backend::new(store).unwrap();
    let mut plan = AnalysisPlan::history(ResultSelection::new(old.experiment), "a");
    let values = |plan: &AnalysisPlan| {
        backend.analyze(plan).unwrap().series[0]
            .points
            .iter()
            .map(|p| p.value)
            .collect::<Vec<_>>()
    };
    assert_eq!(values(&plan), [Some(12.), Some(16.)]);
    assert_eq!(values(&plan), [Some(12.), Some(16.)]);
    plan.metric = Some("b".into());
    assert_eq!(values(&plan), [Some(120.), Some(160.)]);
    plan.selection.inputs = InputSelection::Ids {
        ids: vec![old.inputs[0]],
    };
    assert_eq!(values(&plan), [Some(110.), Some(150.)]);
    plan.selection.revisions = RevisionQuery::Only {
        ids: vec!["b".into()],
    };
    assert_eq!(values(&plan), [Some(150.)]);
    plan.selection.runs = RunSelection::Explicit { ids: vec![old.run] };
    plan.runs = Some(RunTreatment::Pool);
    assert_eq!(values(&plan), [Some(50.)]);
    plan.selection.runs = RunSelection::Explicit { ids: vec![new.run] };
    assert_eq!(values(&plan), [Some(150.)]);
}

#[test]
fn new_completed_measurements_replace_cached_results_and_expire_inspection() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("results.sqlite");
    let mut writer = SqliteStore::open(&path).unwrap();
    let old = register(&mut writer, &[1], &["a"], 1);
    fill(&mut writer, &old, "a", 0, 1., 10.);
    let backend = Backend::open(&path).unwrap();
    let plan = AnalysisPlan::history(ResultSelection::new(old.experiment), "a");
    let before = backend.analyze(&plan).unwrap();
    assert_eq!(before.series[0].points[0].value, Some(1.));
    let newer = register(&mut writer, &[1], &["a"], 1);
    fill(&mut writer, &newer, "a", 0, 2., 20.);
    let after = Backend::open(&path).unwrap().analyze(&plan).unwrap();
    assert_eq!(after.series[0].points[0].value, Some(2.));
    assert!(matches!(
        backend.inspect(
            &before.snapshot,
            &before.series[0].points[0].id,
            Paging::default()
        ),
        Err(StoreError::Expired)
    ));
}
