use super::*;
use crate::{analysis::*, experiment::*, metrics::*};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

mod artifacts;
mod cache;
mod projection;
mod spread;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct Input(u32);
impl BenchmarkIn for Input {
    fn fields() -> Vec<InputField> {
        vec![InputField::new("width", "Width", FieldKind::Number)]
    }
    fn parameters(&self) -> std::collections::BTreeMap<String, serde_json::Value> {
        [("width".into(), serde_json::json!(self.0))].into()
    }
    fn label(&self) -> String {
        format!("Width {}", self.0)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Output {
    a: f64,
    b: f64,
}
#[contracts::contract_trait]
impl BenchmarkOut for Output {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::new("a", "A", MetricType::Float, "s", true),
            MetricDescriptor::new("b", "B", MetricType::Float, "s", true),
        ]
    }
    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "a" => Some(MetricValue::Float(self.a)),
            "b" => Some(MetricValue::Float(self.b)),
            _ => None,
        }
    }
    fn analysis_metrics() -> Vec<MetricSpec> {
        let mut m = crate::metrics::describe(&Self::list_metrics());
        m[0].direction = Direction::Lower;
        for (name, stage) in [
            ("input_ratio", FormulaStage::Input),
            ("suite_ratio", FormulaStage::Suite),
        ] {
            let mut ratio = m[0].clone();
            ratio.path = name.into();
            ratio.unit = "ratio".into();
            ratio.source = MetricSource::Formula {
                numerator: "a".into(),
                denominator: "b".into(),
                formula: FormulaKind::Ratio,
                stage,
            };
            m.push(ratio);
        }
        m
    }
}
struct TestProcedure;
impl Procedure for TestProcedure {
    type Input = Input;
    type Output = Output;
    type Tools = ();
    fn name(&self) -> &str {
        "store-test"
    }
    fn run(
        &self,
        _: &Input,
        _: &(),
        _: &mut RunContext,
    ) -> std::result::Result<Output, ExperimentError> {
        Ok(Output { a: 1., b: 1. })
    }
}
impl StoredProcedure for TestProcedure {
    const STORAGE_KEY: &'static str = "store-test";
    const MEASUREMENT_CONTRACT: &'static str = "test-v1";
}
inventory::submit! {AdapterRegistration::for_procedure::<TestProcedure>()}
fn commit(id: &str) -> crate::src_manager::git::history::Commit {
    serde_json::from_value(serde_json::json!({"id":id,"committed_at":SystemTime::UNIX_EPOCH,"author":"test","commit_message":id})).unwrap()
}
fn definition(inputs: &[u32]) -> SuiteDefinition<Input> {
    SuiteDefinition::new(
        ProcedureId::new("store-test", Version::default()),
        inputs.iter().copied().map(Input).collect(),
    )
    .unwrap()
}
fn register(
    store: &mut SqliteStore,
    inputs: &[u32],
    revs: &[&str],
    repetitions: u32,
) -> RegisteredRun {
    store
        .register_run::<TestProcedure>(
            &ExperimentSpec {
                key: "test".into(),
                name: "Test".into(),
                description: String::new(),
                repository: "test-repo".into(),
            },
            &definition(inputs),
            &RunPlan::new(revs.iter().map(|r| commit(r)).collect(), repetitions),
        )
        .unwrap()
}
fn record(store: &mut SqliteStore, work: WorkItemId, sample: u32, outcome: AttemptOutcome<Output>) {
    let attempt = store.start_attempt(work, sample).unwrap();
    store
        .finish_attempt::<TestProcedure>(
            attempt,
            &AttemptRecord {
                started_at: SystemTime::UNIX_EPOCH,
                duration: Duration::ZERO,
                outcome,
                diagnostics: vec![],
            },
            None,
        )
        .unwrap();
}
fn fill(store: &mut SqliteStore, run: &RegisteredRun, rev: &str, index: usize, a: f64, b: f64) {
    let work = run.work_items[rev][&run.inputs[index]];
    record(store, work, 0, AttemptOutcome::Success(Output { a, b }));
    store.finish_work_item(work).unwrap();
}
#[test]
fn registration_preserves_identity_and_projects_minor_versions() {
    let mut s = SqliteStore::in_memory().unwrap();
    let first = register(&mut s, &[1, 2], &["a"], 1);
    let reordered = register(&mut s, &[2, 1], &["a"], 1);
    assert_eq!(first.definition.id, reordered.definition.id);
    assert_eq!(first.inputs[0], reordered.inputs[1]);
    let extended = register(&mut s, &[3, 2, 1], &["a"], 1);
    assert_eq!(extended.definition.version, Version::new(1, 1));
    fill(&mut s, &extended, "a", 0, 100., 1.);
    fill(&mut s, &extended, "a", 1, 4., 1.);
    fill(&mut s, &extended, "a", 2, 2., 1.);
    let mut selection = ResultSelection::new(first.experiment);
    selection.version = Some(first.definition.version);
    let frame = s.frame::<TestProcedure>(&selection).unwrap();
    assert_eq!(frame.inputs.len(), 2);
    assert_eq!(frame.attempts.len(), 2);
    let backend = Backend::new(s).unwrap();
    let response = backend
        .analyze(&AnalysisPlan::history(selection, "a"))
        .unwrap();
    assert_eq!(response.series[0].points[0].value, Some(3.));
}
#[test]
fn newest_failure_stays_selected_and_unfinished_work_is_reported() {
    let mut s = SqliteStore::in_memory().unwrap();
    let old = register(&mut s, &[1], &["a"], 1);
    fill(&mut s, &old, "a", 0, 1., 1.);
    let failed = register(&mut s, &[1], &["a"], 1);
    s.fail_revision(failed.run, "a", "build failed").unwrap();
    s.finish_run(failed.run).unwrap();
    let pending = register(&mut s, &[1], &["a"], 1);
    let frame = s
        .frame::<TestProcedure>(&ResultSelection::new(old.experiment))
        .unwrap();
    assert_eq!(frame.work_items[0].state, State::BuildFailed);
    assert!(frame.attempts.is_empty());
    assert_eq!(frame.newer_unfinished[0].run, pending.run);
}
#[test]
fn formula_stages_use_matched_operands_and_equal_input_weights() {
    let mut s = SqliteStore::in_memory().unwrap();
    let run = register(&mut s, &[1, 2], &["a"], 1);
    fill(&mut s, &run, "a", 0, 1., 1.);
    fill(&mut s, &run, "a", 1, 9., 3.);
    let backend = Backend::new(s).unwrap();
    let selection = ResultSelection::new(run.experiment);
    let input = backend
        .analyze(&AnalysisPlan::history(selection.clone(), "input_ratio"))
        .unwrap();
    let suite = backend
        .analyze(&AnalysisPlan::history(selection, "suite_ratio"))
        .unwrap();
    assert_eq!(input.series[0].points[0].value, Some(2.));
    assert_eq!(suite.series[0].points[0].value, Some(2.5));
}
#[test]
fn baseline_outside_display_range_and_inspection_expiration() {
    let mut s = SqliteStore::in_memory().unwrap();
    let run = register(&mut s, &[1], &["a", "b"], 1);
    fill(&mut s, &run, "a", 0, 2., 1.);
    fill(&mut s, &run, "b", 0, 3., 1.);
    let mut backend = Backend::new(s).unwrap();
    let mut selection = ResultSelection::new(run.experiment);
    selection.revisions = RevisionQuery::Only {
        ids: vec!["b".into()],
    };
    let mut plan = AnalysisPlan::history(selection, "a");
    plan.normalization = Some(Normalization {
        revision: "a".into(),
        kind: ChangeKind::Ratio,
        runs: None,
    });
    let response = backend.analyze(&plan).unwrap();
    let p = &response.series[0].points[0];
    assert_eq!(p.value, Some(1.5));
    assert_eq!(response.revisions, vec!["b"]);
    let inspected = backend
        .inspect(&response.snapshot, &p.id, Paging::default())
        .unwrap();
    assert_eq!(inspected.attempts.total, 2);
    register(&mut backend.store, &[1], &["a", "b"], 1);
    assert!(matches!(
        backend.inspect(&response.snapshot, &p.id, Paging::default()),
        Err(StoreError::Expired)
    ));
}
#[test]
fn partial_success_is_usable_without_hiding_failures() {
    let mut s = SqliteStore::in_memory().unwrap();
    let run = register(&mut s, &[1], &["a"], 2);
    let work = run.work_items["a"][&run.inputs[0]];
    record(
        &mut s,
        work,
        0,
        AttemptOutcome::Success(Output { a: 4., b: 1. }),
    );
    record(
        &mut s,
        work,
        1,
        AttemptOutcome::Failure("deterministic failure".into()),
    );
    s.finish_work_item(work).unwrap();
    let backend = Backend::new(s).unwrap();
    let result = backend
        .analyze(&AnalysisPlan::history(
            ResultSelection::new(run.experiment),
            "a",
        ))
        .unwrap();
    assert_eq!(result.series[0].points[0].value, Some(4.));
    assert_eq!(result.coverage[0].outcomes.failure, 1);
    assert_eq!(result.series[0].points[0].coverage.valid_repetitions, 1);
}
#[test]
fn reducers_define_sample_sd_and_invalid_domains() {
    assert_eq!(Reduction::StdDev.apply(&[1.]), None);
    assert!((Reduction::StdDev.apply(&[1., 3.]).unwrap() - 2_f64.sqrt()).abs() < 1e-12);
    assert_eq!(Reduction::GeometricMean.apply(&[0., 4.]), None);
    assert_eq!(Reduction::Mean.apply(&[f64::MAX, f64::MAX]), Some(f64::MAX));
}
#[test]
fn changes_scaling_and_named_variation_populations() {
    let mut s = SqliteStore::in_memory().unwrap();
    let run = register(&mut s, &[1, 2], &["a", "b"], 1);
    fill(&mut s, &run, "a", 0, 2., 1.);
    fill(&mut s, &run, "a", 1, 4., 1.);
    fill(&mut s, &run, "b", 0, 3., 1.);
    fill(&mut s, &run, "b", 1, 2., 1.);
    let backend = Backend::new(s).unwrap();
    let mut plan = AnalysisPlan::history(ResultSelection::new(run.experiment), "a");
    plan.operation = Operation::Changes {
        comparison: Comparison::AdjacentSelected,
        threshold: Threshold {
            value: 5.,
            kind: ChangeKind::Percent,
        },
        ranking: Ranking::Percent,
    };
    let changes = backend.analyze(&plan).unwrap();
    assert_eq!(changes.changes.total, 2);
    assert_eq!(changes.changes.items[0].input, run.inputs[0]);
    assert_eq!(changes.change_summaries[0].improved, 1);
    assert_eq!(changes.change_summaries[0].worsened, 1);
    assert_eq!(changes.change_summaries[0].absolute, Some(-0.5));
    plan.operation = Operation::Scaling {
        x: "width".into(),
        reduce_remaining: false,
    };
    let scaling = backend.analyze(&plan).unwrap();
    assert_eq!(scaling.series.len(), 2);
    assert_eq!(scaling.series[0].points[0].x, serde_json::json!(1));
    plan.operation = Operation::Variation {
        population: Population::AcrossInputs,
    };
    let variation = backend.analyze(&plan).unwrap();
    assert_eq!(variation.distributions.total, 2);
    assert_eq!(variation.distributions.items[0].count, 2);
}
#[test]
fn common_coverage_and_fixed_cohorts_keep_their_membership() {
    let mut s = SqliteStore::in_memory().unwrap();
    let run = register(&mut s, &[1, 2], &["a", "b"], 1);
    fill(&mut s, &run, "a", 0, 2., 1.);
    fill(&mut s, &run, "a", 1, 5., 1.);
    fill(&mut s, &run, "b", 0, 3., 1.);
    let backend = Backend::new(s).unwrap();
    let mut plan = AnalysisPlan::history(ResultSelection::new(run.experiment), "a");
    let strict = backend.analyze(&plan).unwrap();
    assert_eq!(strict.series[0].points[1].value, None);
    plan.coverage = CoveragePolicy::Common;
    let common = backend.analyze(&plan).unwrap();
    assert_eq!(common.series[0].points[0].value, Some(2.));
    assert_eq!(common.series[0].points[1].value, Some(3.));
    assert_eq!(common.series[0].points[0].coverage.eligible_inputs, 1);
    plan.cohort = Some(Cohort {
        baseline: "a".into(),
        current: "b".into(),
        threshold: Threshold::default(),
        class: ChangeClass::Worsened,
    });
    let cohort = backend.analyze(&plan).unwrap();
    assert_eq!(cohort.inputs.len(), 1);
    assert_eq!(cohort.inputs[0].id, run.inputs[0]);
    assert_eq!(cohort.series[0].points[0].coverage.selected_inputs, 1);
}
#[test]
fn suite_ratio_keeps_zero_denominator_contributors_until_composition() {
    let mut s = SqliteStore::in_memory().unwrap();
    let run = register(&mut s, &[1, 2], &["a"], 1);
    fill(&mut s, &run, "a", 0, 1., 0.);
    fill(&mut s, &run, "a", 1, 9., 4.);
    let backend = Backend::new(s).unwrap();
    let result = backend
        .analyze(&AnalysisPlan::history(
            ResultSelection::new(run.experiment),
            "suite_ratio",
        ))
        .unwrap();
    assert_eq!(result.series[0].points[0].value, Some(2.5));
    assert_eq!(result.series[0].points[0].coverage.included_inputs, 2);
}
#[test]
fn runner_cleans_each_revision_and_continues_after_build_failure() {
    struct TestSuite;
    impl Suite for TestSuite {
        type Tool = ();
        type Session = ();
        type Procedure = TestProcedure;
        fn name(&self) -> &str {
            "runner-test"
        }
        fn procedure(&self) -> &TestProcedure {
            &TestProcedure
        }
        fn inputs(&self, _: &()) -> std::result::Result<Vec<Input>, ExperimentError> {
            Ok(vec![Input(1)])
        }
        fn begin(&self) -> std::result::Result<(), ExperimentError> {
            Ok(())
        }
        fn tools(&self, _: &(), _: &()) -> std::result::Result<(), ExperimentError> {
            Ok(())
        }
    }
    #[derive(Default)]
    struct Provider {
        active: Option<String>,
        finished: Vec<String>,
    }
    impl ToolProvider for Provider {
        type Tool = ();
        fn revisions(
            &mut self,
            _: &crate::src_manager::git::history::RevisionSelection,
        ) -> std::result::Result<Vec<crate::src_manager::git::history::Commit>, ExperimentError>
        {
            Ok(vec![commit("broken"), commit("working")])
        }
        fn tool(
            &mut self,
            r: &crate::src_manager::git::history::Commit,
        ) -> std::result::Result<(), ExperimentError> {
            assert!(
                self.active.is_none(),
                "previous revision was not cleaned up"
            );
            self.active = Some(r.id().id().clone());
            if r.id().id() == "broken" {
                Err(ExperimentError::Config("build failed".into()))
            } else {
                Ok(())
            }
        }
        fn finish_revision(
            &mut self,
            r: &crate::src_manager::git::history::Commit,
        ) -> std::result::Result<(), ExperimentError> {
            assert_eq!(self.active.take().as_ref(), Some(r.id().id()));
            self.finished.push(r.id().id().clone());
            Ok(())
        }
    }
    let mut store = SqliteStore::in_memory().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let experiment = Experiment::new(
        crate::src_manager::git::history::RevisionSelection::All,
        TestSuite,
        temp.path().into(),
    );
    let mut provider = Provider::default();
    let report = experiment
        .run_stored(
            &mut store,
            &ExperimentSpec {
                key: "runner".into(),
                name: "Runner".into(),
                description: String::new(),
                repository: "test-repo".into(),
            },
            &mut provider,
            &RunOptions {
                perf_counters: None,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(report.revision_failures.len(), 1);
    assert_eq!(report.attempts.len(), 1);
    assert_eq!(provider.finished, ["broken", "working"]);
    assert!(provider.active.is_none());
    assert_eq!(store.run(report.run_id).unwrap().state, State::Complete);
    let states = store.revision_statuses(report.run_id).unwrap();
    assert_eq!(states[0].state, State::BuildFailed);
    assert_eq!(states[1].state, State::Complete);
}

#[test]
fn run_summaries_pooling_and_partitions_preserve_their_populations() {
    let mut store = SqliteStore::in_memory().unwrap();
    let first = register(&mut store, &[1], &["a"], 1);
    fill(&mut store, &first, "a", 0, 1., 1.);
    let second = register(&mut store, &[1], &["a"], 3);
    let work = second.work_items["a"][&second.inputs[0]];
    for sample in 0..3 {
        record(
            &mut store,
            work,
            sample,
            AttemptOutcome::Success(Output { a: 3., b: 1. }),
        );
    }
    store.finish_work_item(work).unwrap();
    let mut backend = Backend::new(store).unwrap();
    let mut selection = ResultSelection::new(first.experiment);
    selection.runs = RunSelection::All;
    let mut plan = AnalysisPlan::history(selection, "a");
    plan.runs = Some(RunTreatment::Summaries {
        reduction: Reduction::Mean,
    });
    assert_eq!(
        backend.analyze(&plan).unwrap().series[0].points[0].value,
        Some(2.)
    );
    plan.runs = Some(RunTreatment::Pool);
    assert_eq!(
        backend.analyze(&plan).unwrap().series[0].points[0].value,
        Some(2.5)
    );
    let mut other_plan = RunPlan::new(vec![commit("a")], 1);
    other_plan.environment.machine = "another-machine".into();
    let other = backend
        .store
        .register_run::<TestProcedure>(
            &ExperimentSpec {
                key: "test".into(),
                name: "Test".into(),
                description: String::new(),
                repository: "test-repo".into(),
            },
            &definition(&[1]),
            &other_plan,
        )
        .unwrap();
    fill(&mut backend.store, &other, "a", 0, 100., 1.);
    plan.selection.runs = RunSelection::LatestCompleted;
    plan.runs = None;
    assert!(backend.analyze(&plan).is_err());
    plan.selection.partitions = PartitionSelection::All;
    assert_eq!(backend.analyze(&plan).unwrap().series.len(), 2);
    plan.selection.run_filters = vec![Filter {
        field: "environment.machine".into(),
        operator: FilterOperator::Equal,
        value: serde_json::json!("another-machine"),
    }];
    assert_eq!(
        backend.analyze(&plan).unwrap().series[0].points[0].value,
        Some(100.)
    );
}

#[test]
fn input_search_paging_and_shared_json_preserve_selection() {
    let mut store = SqliteStore::in_memory().unwrap();
    let run = register(&mut store, &[3, 2, 1], &["a"], 1);
    let backend = Backend::new(store).unwrap();
    let mut selection = ResultSelection::new(run.experiment);
    assert_eq!(
        backend
            .inputs(
                &selection,
                "",
                Paging {
                    offset: 0,
                    limit: 1
                }
            )
            .unwrap()
            .items[0]
            .id,
        run.inputs[0]
    );
    selection.inputs = InputSelection::Matching {
        filters: vec![],
        search: "Width 2".into(),
        excluded: vec![],
    };
    let matches = backend
        .inputs(
            &selection,
            "",
            Paging {
                offset: 0,
                limit: 1,
            },
        )
        .unwrap();
    assert_eq!(matches.total, 1);
    assert_eq!(matches.items[0].id, run.inputs[1]);
    let plan: AnalysisPlan = serde_json::from_value(
        serde_json::json!({"selection":{"experiment":run.experiment},"metric":"a"}),
    )
    .unwrap();
    let result = backend.analyze(&plan).unwrap();
    let json = serde_json::to_string(&result).unwrap();
    let decoded: yosys_bencher_ui_types::analysis::AnalysisResponse =
        serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.inputs[0].id, run.inputs[0]);
}
