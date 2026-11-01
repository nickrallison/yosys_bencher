use super::*;
use crate::api::Metadata;
use serde_json::json;
use std::time::{Duration, SystemTime};
use yosys_bench::{
    analysis::Backend,
    benchmarks::{
        synth::{TimingConfig, TimingProcedure},
        yosys::{YosysBenchmark, YosysBenchmarkInput, YosysBenchmarkOutput},
    },
    experiment::{AttemptOutcome, AttemptRecord, ProcedureId, SuiteDefinition},
    store::{ExperimentSpec, RunPlan, SqliteStore},
    tools::yosys::design::Design,
};
use yosys_bench_ui_types::experiment::*;
fn dataset() -> (Backend, Metadata) {
    type P = YosysBenchmark<TimingProcedure>;
    let mut store = SqliteStore::in_memory().unwrap();
    let inputs = [16, 32]
        .into_iter()
        .map(|width| {
            YosysBenchmarkInput::new(
                Design {
                    top: Some(format!("adder_{width}")),
                    ..Default::default()
                },
                TimingConfig::new(vec![]),
            )
        })
        .collect();
    let definition =
        SuiteDefinition::new(ProcedureId::new("yosys.timing", Version::default()), inputs).unwrap();
    let revisions = ["aaaa1111", "bbbb2222", "cccc3333"]
        .into_iter()
        .enumerate()
        .map(|(i, id)| {
            serde_json::from_value(json!(
                { "id" : id, "committed_at" : SystemTime::UNIX_EPOCH +
                Duration::from_secs(1700000000 + i as u64), "author" : "UI test",
                "commit_message" : format!("Revision {i}") }
            ))
            .unwrap()
        })
        .collect();
    let run = store
        .register_run::<P>(
            &ExperimentSpec {
                key: "timing".into(),
                name: "Timing".into(),
                description: "Test measurements".into(),
                repository: "test".into(),
            },
            &definition,
            &RunPlan::new(revisions, 2),
        )
        .unwrap();
    for (revision, work) in &run.work_items {
        for (input, item) in work {
            for sample in 0..2 {
                let id = store.start_attempt(*item, sample).unwrap();
                let seconds = if revision.starts_with('a') { 1. } else { 2. }
                    + input.0 as f64
                    + sample as f64 * 0.1;
                store
                    .finish_attempt::<P>(
                        id,
                        &AttemptRecord {
                            started_at: SystemTime::UNIX_EPOCH,
                            duration: Duration::from_secs(1),
                            outcome: AttemptOutcome::Success(YosysBenchmarkOutput::new(
                                Duration::from_secs_f64(seconds),
                                (),
                            )),
                            diagnostics: vec![],
                        },
                        None,
                    )
                    .unwrap();
            }
            store.finish_work_item(*item).unwrap();
        }
    }
    store.finish_run(run.run).unwrap();
    let backend = Backend::new(store).unwrap();
    let meta = Metadata {
        description: backend.describe(run.experiment).unwrap(),
        runs: backend.runs(run.experiment).unwrap(),
        revisions: backend
            .revisions(run.experiment, &RevisionQuery::All)
            .unwrap(),
        revision_order_required: false,
        warnings: vec![],
    };
    (backend, meta)
}
#[test]
fn defaults_and_mode_changes_preserve_dataset_choices() {
    let (_, meta) = dataset();
    let mut plan = initial(&meta);
    assert_eq!(plan.metric.as_deref(), Some("runtime"));
    plan.selection.revisions = RevisionQuery::Only {
        ids: vec!["cccc3333".into(), "aaaa1111".into()],
    };
    plan.selection.inputs = InputSelection::Ids {
        ids: vec![InputId(1)],
    };
    let selection = encoded(&plan.selection);
    plan.normalization = Some(Normalization {
        revision: "bbbb2222".into(),
        kind: ChangeKind::Ratio,
        runs: None,
    });
    plan.filters = vec![CalculationFilter {
        stage: FilterStage::Input,
        filter: Filter {
            field: "value".into(),
            operator: FilterOperator::Greater,
            value: json!(0),
        },
    }];
    assert!(switch_mode(&mut plan, "coverage", &meta).is_some());
    assert_eq!(encoded(&plan.selection), selection);
    assert!(plan.normalization.is_none());
    assert!(plan.filters.is_empty());
}
#[test]
fn charts_use_stable_inspection_ids_and_numeric_scaling_axes() {
    let (backend, meta) = dataset();
    let plan = initial(&meta);
    let mut result = backend.analyze(&plan).unwrap();
    let spec = chart(&result, &meta);
    assert_eq!(
        spec.series[0].points[0].row.as_deref(),
        Some(result.series[0].points[0].id.as_str()),
    );
    result.snapshot.plan.operation = Operation::Scaling {
        x: "design".into(),
        reduce_remaining: true,
    };
    for (i, p) in result.series[0].points.iter_mut().enumerate() {
        p.x = json!((i + 1) * 16);
    }
    let spec = chart(&result, &meta);
    assert_eq!(spec.x_type, "linear");
    assert_eq!(spec.series[0].points[0].x, json!(16));
}
#[test]
fn chart_legends_use_input_labels_and_keep_input_ids_for_grouping() {
    let (backend, meta) = dataset();
    let mut plan = initial(&meta);
    plan.series = vec![Dimension::Input];
    let result = backend.analyze(&plan).unwrap();
    let spec = chart(&result, &meta);
    assert_eq!(spec.series.len(), 2);
    for input in &result.inputs {
        let group = result
            .groups
            .iter()
            .find(|g| g.dimensions.get("input") == Some(&json!(input.id)))
            .unwrap();
        let series = spec.series.iter().find(|s| s.id == group.id).unwrap();
        assert_eq!(series.label, format!("input: {}", input.label));
        assert!(series.label.contains("adder_"));
    }

    plan.series.push(Dimension::Parameter("design".into()));
    let result = backend.analyze(&plan).unwrap();
    let spec = chart(&result, &meta);
    for input in &result.inputs {
        assert!(spec.series.iter().any(|s| {
            s.label == format!("design: \"{}\", input: {}", input.label, input.label)
        }));
    }
}
#[test]
fn coverage_counts_input_slots_and_keeps_attempt_counts_separate() {
    let (backend, meta) = dataset();
    let mut plan = initial(&meta);
    plan.operation = Operation::Coverage;
    plan.metric = None;
    let mut result = backend.analyze(&plan).unwrap();
    let mut failed = result.coverage[0].clone();
    failed.outcomes.success = 0;
    failed.outcomes.failure = 2;
    failed.run = Some(RunId(99));
    result.coverage.push(failed);
    let spec = chart(&result, &meta);
    assert_eq!(spec.y_label, "Selected input/environment slots");
    assert!(
        spec.series
            .iter()
            .flat_map(|s| &s.points)
            .all(|p| p.row.is_none())
    );
    let counts: Vec<_> = result
        .revisions
        .iter()
        .map(|r| {
            spec.series
                .iter()
                .flat_map(|s| &s.points)
                .filter(|p| p.x == json!(r))
                .filter_map(|p| p.y)
                .sum::<f64>()
        })
        .collect();
    assert_eq!(counts, vec![2., 2., 2.]);
    assert!(spec.series.iter().any(|s| s.label == "Mixed"));
}
#[derive(Props, Clone, PartialEq)]
struct ViewProps {
    meta: Metadata,
    plan: AnalysisPlan,
    result: AnalysisResponse,
}
fn view(props: ViewProps) -> Element {
    rsx! {
        controls::Sidebar {
            meta: props.meta.clone(),
            plan: props.plan,
            plot: PlotOptions::default(),
            pending: false,
            error: None,
            on_edit: move |_| {},
            on_plot: move |_| {},
            on_apply: move |_| {},
        }
        output::AnalysisOutput {
            meta: props.meta,
            result: Some(props.result),
            plot: PlotOptions::default(),
            loading: false,
            on_select: move |_| {},
            on_page: move |_| {},
        }
    }
}
#[test]
fn all_five_modes_render_their_controls_and_result_tables() {
    let (backend, meta) = dataset();
    for (mode, expected) in [
        ("history", "Normalize to a revision"),
        ("changes", "Practical threshold"),
        ("scaling", "X parameter"),
        ("variation", "Variation population"),
        ("coverage", "Execution and metric coverage"),
    ] {
        let mut plan = initial(&meta);
        switch_mode(&mut plan, mode, &meta);
        let result = backend.analyze(&plan).unwrap();
        let mut dom = VirtualDom::new_with_props(
            view,
            ViewProps {
                meta: meta.clone(),
                plan,
                result,
            },
        );
        dom.rebuild_in_place();
        let html = dioxus::ssr::render(&dom);
        assert!(html.contains(expected), "Missing {expected} in {mode}");
        assert!(html.contains("Update analysis"));
        assert!(html.contains("analysis-plot"));
    }
}
#[test]
fn run_filters_are_used_when_checking_automatic_environment_selection() {
    let (_, mut meta) = dataset();
    let mut other = meta.runs[0].clone();
    other.partition = "other".into();
    other.environment.machine = "other-machine".into();
    meta.runs.push(other);
    let mut plan = initial(&meta);
    assert!(ready(&meta, &plan).is_some());
    plan.selection.run_filters = vec![Filter {
        field: "environment.machine".into(),
        operator: FilterOperator::Equal,
        value: json!("other-machine"),
    }];
    assert!(ready(&meta, &plan).is_none());
}
