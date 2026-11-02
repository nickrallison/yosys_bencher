use crate::{
    api::{self, Metadata},
    filters::Filters,
    inputs::InputPicker,
    model::*,
};
use dioxus::prelude::*;
use yosys_bench_ui_types::{analysis::*, api::ApiError, experiment::*, metrics::*, store::*};
#[derive(Clone)]
pub enum Edit {
    Mode(String),
    Metric(String),
    Version(Version),
    Sources(VersionSources),
    Inputs(InputSelection),
    Revisions(RevisionQuery),
    Partitions(PartitionSelection),
    Runs(RunSelection),
    Treatment(Option<RunTreatment>),
    Repetitions(Reduction),
    InputReduction(Reduction),
    Series(Vec<Dimension>),
    Coverage(CoveragePolicy),
    Normalization(Option<Normalization>),
    Operation(Operation),
    Cohort(Option<Cohort>),
    Calculation(Vec<CalculationFilter>),
    Display(Vec<Filter>),
    RunFilters(Vec<Filter>),
}
pub fn apply(plan: &mut AnalysisPlan, edit: Edit, meta: &Metadata) -> Option<String> {
    plan.paging.offset = 0;
    match edit {
        Edit::Mode(v) => return switch_mode(plan, &v, meta),
        Edit::Metric(v) => {
            plan.metric = if v.is_empty() { None } else { Some(v) };
            if let Some(m) = metric(meta, plan) {
                plan.repetitions = m.default_repetitions;
                plan.inputs = m.default_inputs;
            }
        }
        Edit::Version(v) => {
            plan.selection.version = Some(v);
            plan.selection.inputs = InputSelection::All;
            plan.selection.partitions = PartitionSelection::Automatic;
            plan.selection.runs = RunSelection::LatestCompleted;
            return Some(
                "Workload changed. Input selection and run policy were reset for the new version."
                    .into(),
            );
        }
        Edit::Sources(v) => {
            plan.selection.sources = v;
            plan.selection.partitions = PartitionSelection::Automatic;
        }
        Edit::Inputs(v) => plan.selection.inputs = v,
        Edit::Revisions(v) => plan.selection.revisions = v,
        Edit::Partitions(v) => plan.selection.partitions = v,
        Edit::Runs(v) => plan.selection.runs = v,
        Edit::Treatment(v) => plan.runs = v,
        Edit::Repetitions(v) => plan.repetitions = v,
        Edit::InputReduction(v) => plan.inputs = v,
        Edit::Series(v) => {
            if v.iter().any(|d| matches!(d, Dimension::Run)) {
                plan.runs = Some(RunTreatment::Separate);
            }
            plan.series = v;
        }
        Edit::Coverage(v) => plan.coverage = v,
        Edit::Normalization(v) => plan.normalization = v,
        Edit::Operation(v) => plan.operation = v,
        Edit::Cohort(v) => plan.cohort = v,
        Edit::Calculation(v) => plan.filters = v,
        Edit::Display(v) => plan.display_filters = v,
        Edit::RunFilters(v) => plan.selection.run_filters = v,
    }
    None
}
#[component]
pub fn Choice(
    label: String,
    value: String,
    options: Vec<(String, String)>,
    #[props(default)] disabled: bool,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "field",
            "{label}"
            select {
                aria_label: label.clone(),
                value,
                disabled,
                onchange: move |e| on_change.call(e.value()),
                for (value, text) in options {
                    option { value, "{text}" }
                }
            }
        }
    }
}
#[component]
fn Toggle(
    label: String,
    value: bool,
    #[props(default)] disabled: bool,
    on_change: EventHandler<bool>,
) -> Element {
    rsx! {
        label { class: "check-row",
            input {
                r#type: "checkbox",
                aria_label: label
                        .clone(),
                checked: value,
                disabled,
                onchange: move | e | on_change.call(e
                        .checked()),
            }
            span { "{label}" }
        }
    }
}
#[component]
fn Reduce(
    label: String,
    value: Reduction,
    options: Vec<Reduction>,
    #[props(default)] disabled: bool,
    on_change: EventHandler<Reduction>,
) -> Element {
    rsx! {
        Choice {
            label,
            value: encoded(&value),
            options: options.into_iter().map(| r |
                    (encoded(& r), reduction(r).into())).collect(),
            disabled,
            on_change: move |v: String| {
                if let Some(r) = decoded(&v) {
                    on_change.call(r);
                }
            },
        }
    }
}
#[component]
pub fn Sidebar(
    meta: Metadata,
    plan: AnalysisPlan,
    plot: PlotOptions,
    pending: bool,
    error: Option<ApiError>,
    on_edit: EventHandler<Edit>,
    on_plot: EventHandler<PlotOptions>,
    on_apply: EventHandler<()>,
) -> Element {
    let m = metric(&meta, &plan).cloned();
    let is_coverage = matches!(plan.operation, Operation::Coverage);
    let error_field = error.as_ref().and_then(|e| e.field.clone());
    rsx! {
        aside { class: "analysis-sidebar", aria_label: "Analysis editor",
            section { class: "editor-section", id: "metric-controls",
                h2 { "Analysis" }
                Choice {
                    label: "Analysis",
                    value: mode(&plan.operation),
                    options: vec![
                        ("history".into(), "History".into()),
                        ("changes".into(), "Changes".into()),
                        ("scaling".into(), "Scaling".into()),
                        ("variation".into(), "Variation".into()),
                        ("coverage".into(), "Coverage".into()),
                    ],
                    on_change: move |v| on_edit.call(Edit::Mode(v)),
                }
                crate::metric_picker::MetricPicker {
                    metrics: meta.description.metrics.clone(),
                    value: plan.metric.clone(),
                    allow_none: is_coverage,
                    on_change: move |v| on_edit.call(Edit::Metric(v)),
                }
                if let Some(m) = &m {
                    p { class: "muted", "{m.description}" }
                    p { class: "muted",
                        match m.direction {
                            Direction::Lower => "Lower is better.",
                            Direction::Higher => "Higher is better.",
                            Direction::Unspecified => "No improvement direction is declared.",
                        }
                    }
                }
            }
            section { class: "editor-section", id: "data-controls",
                h2 { "Data" }
                Choice {
                    label: "Version sources",
                    value: encoded(&plan.selection.sources),
                    options: vec![
                        (
                            encoded(&VersionSources::CompatibleSupersets),
                            "Compatible minor versions".into(),
                        ),
                        (encoded(&VersionSources::Exact), "Exact version only".into()),
                    ],
                    on_change: move |v: String| {
                        if let Some(v) = decoded(&v) {
                            on_edit.call(Edit::Sources(v));
                        }
                    },
                }
                InputPicker {
                    meta: meta.clone(),
                    selection: plan.selection.clone(),
                    on_edit,
                }
                CommitPicker {
                    meta: meta.clone(),
                    value: plan.selection.revisions.clone(),
                    on_change: move | v | on_edit
                            .call(Edit::Revisions(v)),
                }
            }
            section { class: "editor-section", id: "run-controls",
                h2 { "Runs and environments" }
                RunControls { meta: meta.clone(), plan: plan.clone(), on_edit }
            }
            section { class: "editor-section", id: "operation-controls",
                h2 {
                    match plan.operation {
                        Operation::History => "Relative history",
                        Operation::Changes { .. } => "Find changes",
                        Operation::Scaling { .. } => "Parameter scaling",
                        Operation::Variation { .. } => "Variation",
                        Operation::Coverage => "Coverage",
                    }
                }
                ModeControls { meta: meta
                            .clone(), plan: plan.clone(), on_edit }
            }
            if !is_coverage {
                section { class: "editor-section", id: "summary-controls",
                    h2 { "Summarize" }
                    if let Some(m) = &m {
                        Reduce {
                            label: "Repetitions",
                            value: plan.repetitions,
                            options: m
                                    .repetitions.clone(),
                            on_change: move |r| on_edit.call(Edit::Repetitions(r)),
                        }
                        if !matches!(
                            plan.operation,
                            Operation::Variation {
                                population: Population::WithinRun | Population::BetweenRuns,
                            }
                        )
                        {
                            Reduce {
                                label: "Inputs",
                                value: plan.inputs,
                                options: {
                                    let mut values = m.inputs.clone();
                                    if plan.normalization.as_ref().is_some_and(|n| n.kind == ChangeKind::Ratio)
                                        && !values.contains(&Reduction::GeometricMean)
                                    {
                                        values.push(Reduction::GeometricMean);
                                    }
                                    values
                                },
                                disabled: plan.series.iter().any(|d| matches!(d, Dimension::Input)),
                                on_change: move |r| on_edit.call(Edit::InputReduction(r)),
                            }
                        }
                        if plan.inputs == Reduction::StdDev {
                            p { class: "muted", "This plots differences between inputs as the value. Use Mean for the central estimate." }
                            button {
                                r#type: "button",
                                onclick: move |_| {
                                    on_edit.call(Edit::InputReduction(Reduction::Mean));
                                },
                                "Use mean"
                            }
                        }
                        if let MetricSource::Formula { numerator, denominator, stage, .. } = &m.source {
                            p { class: "muted",
                                "Formula operands: {label(numerator)} and {label(denominator)}. "
                                match stage {
                                    FormulaStage::Repetition => "Computed per repetition.",
                                    FormulaStage::Input => "Computed after per-input estimation.",
                                    FormulaStage::Suite => "Computed after combining matched inputs.",
                                }
                            }
                        }
                    }
                    GroupControls { meta: meta
                                .clone(), plan: plan.clone(), on_edit }
                    p { class: "muted", "Reduce repetitions first; each input has equal weight." }
                }
            }
            section { class: "editor-section", id: "coverage-controls",
                h2 { "Coverage policy" }
                Choice {
                    label: "Coverage policy",
                    value: encoded(&plan.coverage),
                    disabled: is_coverage,
                    options: vec![
                        (encoded(&CoveragePolicy::Strict), "Require every selected input".into()),
                        (encoded(&CoveragePolicy::Common), "Fixed common input set".into()),
                        (encoded(&CoveragePolicy::Available), "Available inputs per revision".into()),
                    ],
                    on_change: move |v: String| {
                        if let Some(v) = decoded(&v) {
                            on_edit.call(Edit::Coverage(v));
                        }
                    },
                }
                p { class: "muted",
                    match plan.coverage {
                        CoveragePolicy::Strict => "Missing required inputs produce chart gaps.",
                        CoveragePolicy::Common => "Every revision uses the same matched inputs.",
                        CoveragePolicy::Available => {
                            "The inputs contributing to each revision may differ."
                        }
                    }
                }
            }
            if !is_coverage {
                details { class: "editor-section",
                    summary { "Filters and change cohorts" }
                    AdvancedControls { meta: meta.clone(), plan: plan.clone(), on_edit }
                }
            }
            section { class: "editor-section", id: "plot-controls",
                h2 { "Plot" }
                PlotControls { plot, operation: plan.operation.clone(), on_change: on_plot }
            }
            div { class: "apply-controls",
                if let Some(field) = error_field {
                    p { class: "error", "Check {field}." }
                }
                button { class: "primary", onclick: move |_| on_apply.call(()), "Update analysis" }
                if pending {
                    p { class: "muted", role: "status", "Changes pending" }
                }
            }
        }
    }
}
fn revision_options(meta: &Metadata) -> Vec<(String, String)> {
    meta.revisions
        .iter()
        .map(|r| {
            (
                r.id.clone(),
                format!(
                    "{} {}",
                    short(&r.id),
                    r.message
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .chars()
                        .take(65)
                        .collect::<String>(),
                ),
            )
        })
        .collect()
}
#[component]
pub fn RevisionField(
    label: String,
    value: String,
    meta: Metadata,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        Choice {
            label,
            value,
            options: revision_options(&meta),
            on_change,
        }
    }
}
#[component]
fn CommitPicker(
    meta: Metadata,
    value: RevisionQuery,
    on_change: EventHandler<RevisionQuery>,
) -> Element {
    let mode = match &value {
        RevisionQuery::All => "all",
        RevisionQuery::Range { .. } => "range",
        RevisionQuery::Only { .. } => "only",
    };
    let (start, end) = match &value {
        RevisionQuery::Range { start, end } => (start.clone(), end.clone()),
        _ => (None, None),
    };
    let mut starts = vec![(String::new(), "First recorded revision".into())];
    starts.extend(revision_options(&meta));
    let mut ends = vec![(String::new(), "Last recorded revision".into())];
    ends.extend(revision_options(&meta));
    rsx! {
        Choice {
            label: "Revisions",
            value: mode,
            options: vec![
                ("all".into(), "All recorded revisions".into()),
                ("range".into(), "Revision range".into()),
                ("only".into(), "Choose revisions and order".into()),
            ],
            on_change: move |v: String| {
                on_change
                    .call(
                        match v.as_str() {
                            "range" => {
                                RevisionQuery::Range {
                                    start: None,
                                    end: None,
                                }
                            }
                            "only" => RevisionQuery::Only { ids: vec![] },
                            _ => RevisionQuery::All,
                        },
                    )
            },
        }
        if mode == "range" {
            Choice {
                label: "From",
                value: start.clone().unwrap_or_default(),
                options: starts,
                on_change: {
                    let end = end.clone();
                    move |v: String| {
                        on_change
                            .call(RevisionQuery::Range {
                                start: (!v.is_empty()).then_some(v),
                                end: end.clone(),
                            })
                    }
                },
            }
            Choice {
                label: "Through",
                value: end.unwrap_or_default(),
                options: ends,
                on_change: move |v: String| {
                    on_change
                        .call(RevisionQuery::Range {
                            start: start.clone(),
                            end: (!v.is_empty()).then_some(v),
                        })
                },
            }
        }
        if let RevisionQuery::Only { ids } = value {
            details { class: "picker",
                summary { "{ids.len()} selected revisions" }
                div { class: "picker-body",
                    p { class: "muted", "Selection order determines the X axis." }
                    div { class: "picker-list",
                        for revision in &meta.revisions {
                            label { class: "check-row",
                                input {
                                    r#type: "checkbox",
                                    checked: ids.contains(&revision.id),
                                    onchange: {
                                        let mut next = ids.clone();
                                        let id = revision.id.clone();
                                        move |_| {
                                            if next.contains(&id) {
                                                next.retain(|v| v != &id);
                                            } else {
                                                next.push(id.clone());
                                            }
                                            on_change
                                                .call(RevisionQuery::Only {
                                                    ids: next.clone(),
                                                });
                                        }
                                    },
                                }
                                span {
                                    "{short(&revision.id)}"
                                    small { "{revision.message.lines().next().unwrap_or_default()}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn RunControls(meta: Metadata, plan: AnalysisPlan, on_edit: EventHandler<Edit>) -> Element {
    let mut choices = vec![
        (String::new(), "Automatic (one configuration)".into()),
        ("all".into(), "All configurations, separate series".into()),
    ];
    choices.extend(partitions(&meta, &plan));
    let selected = match &plan.selection.partitions {
        PartitionSelection::Automatic => String::new(),
        PartitionSelection::All => "all".into(),
        PartitionSelection::Only { ids } => ids.first().cloned().unwrap_or_default(),
    };
    let run_mode = match &plan.selection.runs {
        RunSelection::LatestCompleted => "latest",
        RunSelection::All => "all",
        RunSelection::Explicit { .. } => "explicit",
    };
    let treatment = match &plan.runs {
        None => "",
        Some(RunTreatment::Separate) => "separate",
        Some(RunTreatment::Pool) => "pool",
        Some(RunTreatment::Summaries { .. }) => "summaries",
    };
    let runs = eligible_runs(&meta, &plan);
    let configuration = runs
        .iter()
        .find(|r| {
            selected == r.partition || (selected.is_empty() && partitions(&meta, &plan).len() == 1)
        })
        .map(|r| encoded(&(&r.environment, &r.options)));
    rsx! {
        Choice {
            label: "Environment / configuration",
            value: selected,
            options: choices,
            on_change: move |v: String| {
                on_edit
                    .call(
                        Edit::Partitions(
                            match v.as_str() {
                                "" => PartitionSelection::Automatic,
                                "all" => PartitionSelection::All,
                                _ => {
                                    PartitionSelection::Only {
                                        ids: vec![v],
                                    }
                                }
                            },
                        ),
                    )
            },
        }
        if let Some(configuration) = configuration {
            details { class: "more-options",
                summary { "Configuration details" }
                pre { class: "metadata-json", "{configuration}" }
            }
        }
        Choice {
            label: "Run selection",
            value: run_mode,
            options: vec![
                ("latest".into(), "Latest completed work item".into()),
                ("explicit".into(), "Choose runs".into()),
                ("all".into(), "All matching runs".into()),
            ],
            on_change: move |v: String| {
                on_edit
                    .call(
                        Edit::Runs(
                            match v.as_str() {
                                "explicit" => {
                                    RunSelection::Explicit {
                                        ids: vec![],
                                    }
                                }
                                "all" => RunSelection::All,
                                _ => RunSelection::LatestCompleted,
                            },
                        ),
                    )
            },
        }
        if let RunSelection::Explicit { ids } = &plan.selection.runs {
            details { class: "picker",
                summary { "{ids.len()} runs selected" }
                div { class: "picker-body",
                    div { class: "picker-list",
                        for run in runs {
                            label { class: "check-row",
                                input {
                                    r#type: "checkbox",
                                    checked: ids.contains(&run.id),
                                    onchange: {
                                        let mut ids = ids.clone();
                                        let id = run.id;
                                        move |_| {
                                            if ids.contains(&id) { ids.retain(|v| *v != id) } else { ids.push(id) }
                                            on_edit
                                                .call(
                                                    Edit::Runs(RunSelection::Explicit {
                                                        ids: ids.clone(),
                                                    }),
                                                );
                                        }
                                    },
                                }
                                span {
                                    "Run {run.id}"
                                    small {
                                        "{run.environment.machine} · {run.state:?} · v{version(run.version)}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if !matches!(plan.selection.runs, RunSelection::LatestCompleted) {
            Choice {
                label: "Multiple runs",
                value: treatment,
                disabled: plan.series.iter().any(|
                        d | matches!(d, Dimension::Run)),
                options: vec![
                    ("".into(), "Choose run treatment".into()),
                    ("separate".into(), "Separate series per run".into()),
                    ("summaries".into(), "Reduce per-run estimates".into()),
                    ("pool".into(), "Pool repetitions explicitly".into()),
                ],
                on_change: move |v: String| {
                    on_edit
                        .call(
                            Edit::Treatment(
                                match v.as_str() {
                                    "separate" => Some(RunTreatment::Separate),
                                    "summaries" => {
                                        Some(RunTreatment::Summaries {
                                            reduction: Reduction::Mean,
                                        })
                                    }
                                    "pool" => Some(RunTreatment::Pool),
                                    _ => None,
                                },
                            ),
                        )
                },
            }
            if let Some(RunTreatment::Summaries { reduction: r }) = plan.runs {
                if let Some(m) = metric(&meta, &plan) {
                    Reduce {
                        label: "Run summaries",
                        value: r,
                        options: m.runs.clone(),
                        on_change: move |r| {
                            on_edit
                                .call(
                                    Edit::Treatment(
                                        Some(RunTreatment::Summaries {
                                            reduction: r,
                                        }),
                                    ),
                                )
                        },
                    }
                }
            }
        }
        details { class: "more-options",
            summary { "Run metadata filters" }
            Filters {
                fields: meta.description.run_fields.clone(),
                filters: plan.selection
                        .run_filters.clone(),
                on_change: move |v| on_edit.call(Edit::RunFilters(v)),
            }
        }
    }
}
#[component]
fn ThresholdControl(
    label: String,
    value: Threshold,
    on_change: EventHandler<Threshold>,
) -> Element {
    rsx! {
        Choice {
            label: format!("{label} units"),
            value: encoded(&value.kind),
            options: vec![
                (encoded(&ChangeKind::Percent), "Relative percent".into()),
                (encoded(&ChangeKind::Absolute), "Absolute metric units".into()),
            ],
            on_change: move |v: String| {
                if let Some(kind) = decoded(&v) {
                    on_change.call(Threshold { kind, ..value });
                }
            },
        }
        label { class: "field",
            "{label}"
            input {
                aria_label: label.clone(),
                r#type: "number",
                min: "0",
                step: "any",
                value: value.value,
                onchange: move |e| {
                    if let Ok(v) = e.value().parse::<f64>() && v.is_finite() && v >= 0. {
                        on_change.call(Threshold { value: v, ..value });
                    }
                },
            }
        }
    }
}
#[component]
fn ModeControls(meta: Metadata, plan: AnalysisPlan, on_edit: EventHandler<Edit>) -> Element {
    let first = meta
        .revisions
        .first()
        .map(|r| r.id.clone())
        .unwrap_or_default();
    let last = meta
        .revisions
        .last()
        .map(|r| r.id.clone())
        .unwrap_or_default();
    let suite = metric(&meta, &plan).is_some_and(|m| {
        matches!(
            m.source,
            MetricSource::Formula {
                stage: FormulaStage::Suite,
                ..
            }
        )
    });
    match plan.operation.clone() {
        Operation::History | Operation::Scaling { .. } => {
            rsx! {
                if let Operation::Scaling { x, reduce_remaining } = plan.operation {
                    Choice {
                        label: "X parameter",
                        value: x.clone(),
                        options: meta
                                        .description.fields.iter().map(| f | (f.path.clone(), f.label.clone()))
                                        .collect(),
                        on_change: move |x| {
                            on_edit
                                .call(
                                    Edit::Operation(Operation::Scaling {
                                        x,
                                        reduce_remaining,
                                    }),
                                )
                        },
                    }
                    Toggle {
                        label: "Combine remaining varying parameters",
                        value: reduce_remaining,
                        on_change: move |reduce_remaining| {
                            on_edit
                                .call(
                                    Edit::Operation(Operation::Scaling {
                                        x: x.clone(),
                                        reduce_remaining,
                                    }),
                                )
                        },
                    }
                    p { class: "muted",
                        "Otherwise fix remaining parameters with input filters or keep them as series."
                    }
                }
                Toggle {
                    label: "Normalize to a revision",
                    value: plan
                                    .normalization.is_some(),
                    disabled: suite,
                    on_change: {
                        let baseline = first;
                        move |enabled: bool| {
                            on_edit
                                .call(
                                    Edit::Normalization(
                                        enabled
                                            .then(|| Normalization {
                                                revision: baseline.clone(),
                                                kind: ChangeKind::Ratio,
                                                runs: None,
                                            }),
                                    ),
                                )
                        }
                    },
                }
                if let Some(n) = plan.normalization {
                    RevisionField {
                        label: "Baseline revision",
                        value: n
                                        .revision.clone(),
                        meta: meta.clone(),
                        on_change: {
                            let n = n.clone();
                            move |revision| {
                                on_edit
                                    .call(
                                        Edit::Normalization(
                                            Some(Normalization {
                                                revision,
                                                ..n.clone()
                                            }),
                                        ),
                                    )
                            }
                        },
                    }
                    Choice {
                        label: "Normalization",
                        value: encoded(&n.kind),
                        options: vec![
                            (encoded(&ChangeKind::Ratio), "Ratio to baseline".into()),
                            (encoded(&ChangeKind::Percent), "Percent change from baseline".into()),
                            (encoded(&ChangeKind::Absolute), "Absolute difference".into()),
                        ],
                        on_change: {
                            let n = n.clone();
                            move |v: String| {
                                if let Some(kind) = decoded(&v) {
                                    on_edit
                                        .call(
                                            Edit::Normalization(Some(Normalization { kind, ..n.clone() })),
                                        );
                                }
                            }
                        },
                    }
                    p { class: "muted",
                        "Normalize each input before combining inputs. The baseline may be outside the visible range."
                    }
                }
                if suite {
                    p { class: "muted",
                        "Suite-level formulas use matched operands after input reduction; revision normalization is unavailable."
                    }
                }
            }
        }
        Operation::Changes {
            comparison,
            threshold,
            ranking,
        } => {
            let comparison_mode = match &comparison {
                Comparison::Pair { .. } => "pair",
                Comparison::AdjacentSelected => "adjacent",
                Comparison::PreviousMeasured => "measured",
            };
            rsx! {
                Choice {
                    label: "Comparison",
                    value: comparison_mode,
                    options: vec![
                        ("adjacent".into(), "Adjacent selected revisions".into()),
                        ("pair".into(), "Explicit revision pair".into()),
                        ("measured".into(), "Previous measured revision".into()),
                    ],
                    on_change: move |v: String| {
                        on_edit
                            .call(
                                Edit::Operation(Operation::Changes {
                                    comparison: match v.as_str() {
                                        "pair" => {
                                            Comparison::Pair {
                                                baseline: first.clone(),
                                                current: last.clone(),
                                            }
                                        }
                                        "measured" => Comparison::PreviousMeasured,
                                        _ => Comparison::AdjacentSelected,
                                    },
                                    threshold,
                                    ranking,
                                }),
                            )
                    },
                }
                if let Comparison::Pair { baseline, current } = &comparison {
                    RevisionField {
                        label: "Before revision",
                        value: baseline.clone(),
                        meta: meta.clone(),
                        on_change: {
                            let current = current.clone();
                            move |baseline| {
                                on_edit
                                    .call(
                                        Edit::Operation(Operation::Changes {
                                            comparison: Comparison::Pair {
                                                baseline,
                                                current: current.clone(),
                                            },
                                            threshold,
                                            ranking,
                                        }),
                                    )
                            }
                        },
                    }
                    RevisionField {
                        label: "After revision",
                        value: current.clone(),
                        meta: meta.clone(),
                        on_change: {
                            let baseline = baseline.clone();
                            move |current| {
                                on_edit
                                    .call(
                                        Edit::Operation(Operation::Changes {
                                            comparison: Comparison::Pair {
                                                baseline: baseline.clone(),
                                                current,
                                            },
                                            threshold,
                                            ranking,
                                        }),
                                    )
                            }
                        },
                    }
                }
                ThresholdControl {
                    label: "Practical threshold",
                    value: threshold,
                    on_change: {
                        let comparison = comparison.clone();
                        move |threshold| {
                            on_edit
                                .call(
                                    Edit::Operation(Operation::Changes {
                                        comparison: comparison.clone(),
                                        threshold,
                                        ranking,
                                    }),
                                )
                        }
                    },
                }
                Choice {
                    label: "Rank by",
                    value: encoded(&ranking),
                    options: vec![
                        (encoded(&Ranking::Percent), "Relative change (%)".into()),
                        (encoded(&Ranking::Absolute), "Absolute change".into()),
                        (encoded(&Ranking::Contribution), "Contribution to suite change".into()),
                    ],
                    on_change: move |v: String| {
                        if let Some(ranking) = decoded(&v) {
                            on_edit
                                .call(
                                    Edit::Operation(Operation::Changes {
                                        comparison: comparison.clone(),
                                        threshold,
                                        ranking,
                                    }),
                                );
                        }
                    },
                }
                p { class: "muted",
                    "Ranked from largest regression. A practical threshold does not establish statistical significance. Contributions require an arithmetic mean."
                }
            }
        }
        Operation::Variation { population } => {
            rsx! {
                Choice {
                    label: "Variation population",
                    value: encoded(&population),
                    options: vec![
                        (encoded(&Population::WithinRun), "Repetitions within a run".into()),
                        (encoded(&Population::BetweenRuns), "Per-run estimates".into()),
                        (encoded(&Population::AcrossInputs), "Input estimates (workload spread)".into()),
                    ],
                    on_change: move |v: String| {
                        if let Some(population) = decoded(&v) {
                            on_edit.call(Edit::Operation(Operation::Variation { population }));
                        }
                    },
                }
                p { class: "muted",
                    "Sample SD is descriptive spread, not a confidence interval. Fewer than two observations cannot establish variation."
                }
            }
        }
        Operation::Coverage => {
            rsx! {
                p { class: "muted",
                    "Track measured, failed, unsupported, unfinished, and unmeasured inputs. Optional metric availability is separate from execution state. This view keeps the selected workload as its denominator."
                }
            }
        }
    }
}
#[component]
fn GroupControls(meta: Metadata, plan: AnalysisPlan, on_edit: EventHandler<Edit>) -> Element {
    let mut options = vec![
        (Dimension::Input, "Input identity".into()),
        (Dimension::Run, "Run".into()),
        (Dimension::Environment, "Environment".into()),
    ];
    options.extend(
        meta.description
            .fields
            .iter()
            .filter(|f| f.groupable)
            .map(|f| (Dimension::Parameter(f.path.clone()), f.label.clone())),
    );
    options.extend(meta.description.run_fields.iter().filter_map(|f| {
        f.path
            .strip_prefix("option:")
            .map(|p| (Dimension::Option(p.into()), f.label.clone()))
    }));
    let selected = plan.series.clone();
    let preview_field = plan.series.iter().find_map(|d| {
        if let Dimension::Parameter(p) = d {
            Some(p.clone())
        } else {
            None
        }
    });
    let fetch_selection = plan.selection.clone();
    let mut preview = use_reactive!(|(preview_field, fetch_selection)| (
        preview_field.clone(),
        fetch_selection.clone()
    ));
    let counts = use_resource(move || {
        let (field, selection) = preview();
        async move {
            if let Some(field) = field {
                Some((
                    field.clone(),
                    api::result(api::groups(selection, field).await),
                ))
            } else {
                None
            }
        }
    });
    rsx! {
        details { class: "picker",
            summary {
                if selected.is_empty() {
                    "Combined series"
                } else {
                    "{selected.len()} series dimensions"
                }
            }
            div { class: "picker-body",
                div { class: "picker-list",
                    for (dimension, name) in options {
                        label { class: "check-row",
                            input {
                                r#type: "checkbox",
                                checked: selected.contains(&
                                        dimension),
                                onchange: {
                                    let mut next = selected.clone();
                                    move |_| {
                                        if next.contains(&dimension) {
                                            next.retain(|d| *d != dimension)
                                        } else {
                                            next.push(dimension.clone());
                                        }
                                        on_edit.call(Edit::Series(next.clone()));
                                    }
                                },
                            }
                            span { "{name}" }
                        }
                    }
                }
            }
        }
        if let Some(Some((field, Ok(groups)))) = counts.read().as_ref() {
            if plan.series.iter().any(|d| matches!(d, Dimension::Parameter(p) if p == field)) {
                p { class: "muted", "{groups.len()} groups for {label(field)}" }
                if groups.len() > 30 {
                    p { class: "notice compact",
                        "This creates many series. Narrow the workload before updating."
                    }
                }
                details { class: "more-options",
                    summary { "Preview group values" }
                    ul { class: "group-preview",
                        for (value, count) in groups.iter().take(20) {
                            li { "{value_text(value)}: {count} inputs" }
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn AdvancedControls(meta: Metadata, plan: AnalysisPlan, on_edit: EventHandler<Edit>) -> Element {
    let mut attempt = Vec::new();
    let mut input = Vec::new();
    for f in &plan.filters {
        match f.stage {
            FilterStage::Attempt => attempt.push(f.filter.clone()),
            FilterStage::Input => input.push(f.filter.clone()),
        }
    }
    let attempt_rest = plan
        .filters
        .iter()
        .filter(|f| !matches!(f.stage, FilterStage::Attempt))
        .cloned()
        .collect::<Vec<_>>();
    let input_rest = plan
        .filters
        .iter()
        .filter(|f| !matches!(f.stage, FilterStage::Input))
        .cloned()
        .collect::<Vec<_>>();
    let suite = metric(&meta, &plan).is_some_and(|m| {
        matches!(
            m.source,
            MetricSource::Formula {
                stage: FormulaStage::Suite,
                ..
            }
        )
    });
    let direction = metric(&meta, &plan)
        .map(|m| m.direction)
        .unwrap_or(Direction::Unspecified);
    let first = meta
        .revisions
        .first()
        .map(|r| r.id.clone())
        .unwrap_or_default();
    let last = meta
        .revisions
        .last()
        .map(|r| r.id.clone())
        .unwrap_or_default();
    rsx! {
        p { class: "muted", "Calculation filters change estimates and retain exclusion reasons." }
        h3 { class: "small-heading", "Attempt outcome" }
        Filters {
            fields: vec![InputField::new("outcome", "Outcome", FieldKind::Text)],
            filters: attempt,
            on_change: move |filters: Vec<Filter>| {
                let mut next = attempt_rest.clone();
                next.extend(
                    filters
                        .into_iter()
                        .map(|filter| CalculationFilter {
                            stage: FilterStage::Attempt,
                            filter,
                        }),
                );
                on_edit.call(Edit::Calculation(next));
            },
        }
        p { class: "muted", "Outcome values: success, failure, unsupported." }
        if !suite {
            h3 { class: "small-heading", "Per-input estimate, before normalization" }
            Filters {
                fields: vec![InputField::new("value", "Measurement", FieldKind::Number)],
                filters: input,
                on_change: move |filters: Vec<Filter>| {
                    let mut next = input_rest.clone();
                    next.extend(
                        filters
                            .into_iter()
                            .map(|filter| CalculationFilter {
                                stage: FilterStage::Input,
                                filter,
                            }),
                    );
                    on_edit.call(Edit::Calculation(next));
                },
            }
        }
        h3 { class: "small-heading", "Display only" }
        p { class: "muted",
            "Hide rows or points without changing calculations. Uses the ranking value for Changes and mean for Variation."
        }
        Filters {
            fields: vec![InputField::new("value", "Displayed value", FieldKind::Number)],
            filters: plan.display_filters.clone(),
            on_change: move | v
                    | on_edit.call(Edit::Display(v)),
        }
        h3 { class: "small-heading", "Fixed change cohort" }
        Toggle {
            label: "Focus on changed inputs",
            value: plan
                    .cohort.is_some(),
            disabled: suite || direction == Direction::Unspecified,
            on_change: move |enabled: bool| {
                on_edit
                    .call(
                        Edit::Cohort(
                            enabled
                                .then(|| Cohort {
                                    baseline: first.clone(),
                                    current: last.clone(),
                                    threshold: Threshold {
                                        value: 5.,
                                        kind: ChangeKind::Percent,
                                    },
                                    class: ChangeClass::Worsened,
                                }),
                        ),
                    )
            },
        }
        if let Some(c) = plan.cohort {
            RevisionField {
                label: "Cohort baseline",
                value: c.baseline.clone(),
                meta: meta.clone(),
                on_change: {
                    let c = c.clone();
                    move |baseline| {
                        on_edit.call(Edit::Cohort(Some(Cohort { baseline, ..c.clone() })))
                    }
                },
            }
            RevisionField {
                label: "Cohort comparison",
                value: c.current.clone(),
                meta: meta.clone(),
                on_change: {
                    let c = c.clone();
                    move |current| on_edit.call(Edit::Cohort(Some(Cohort { current, ..c.clone() })))
                },
            }
            ThresholdControl {
                label: "Cohort threshold",
                value: c.threshold,
                on_change: {
                    let c = c.clone();
                    move |threshold| {
                        on_edit.call(Edit::Cohort(Some(Cohort { threshold, ..c.clone() })))
                    }
                },
            }
            Choice {
                label: "Cohort inputs",
                value: encoded(&c.class),
                options: vec![
                    (encoded(&ChangeClass::Worsened), "Worsened".into()),
                    (encoded(&ChangeClass::Improved), "Improved".into()),
                    (encoded(&ChangeClass::WithinThreshold), "Within threshold".into()),
                ],
                on_change: move |v: String| {
                    if let Some(class) = decoded(&v) {
                        on_edit.call(Edit::Cohort(Some(Cohort { class, ..c.clone() })));
                    }
                },
            }
            p { class: "muted",
                "The same matched input identities are used throughout the displayed history."
            }
        }
    }
}
#[component]
fn PlotControls(
    plot: PlotOptions,
    operation: Operation,
    on_change: EventHandler<PlotOptions>,
) -> Element {
    let scale = plot.clone();
    let min = plot.clone();
    let max = plot.clone();
    let lines = plot.clone();
    let spread = plot.clone();
    let range = plot.clone();
    rsx! {
        Choice {
            label: "Y scale",
            value: encoded(&plot.y_scale),
            options: vec![
                (encoded(&AxisScale::Linear), "Linear".into()),
                (encoded(&AxisScale::Log), "Logarithmic".into()),
            ],
            on_change: move |v: String| {
                if let Some(y_scale) = decoded(&v) {
                    on_change
                        .call(PlotOptions {
                            y_scale,
                            ..scale.clone()
                        });
                }
            },
        }
        if matches!(operation, Operation::History | Operation::Scaling { .. } | Operation::Variation { .. }) {
            if !matches!(operation, Operation::Variation { .. }) {
                p { class: "muted", "Spread uses repetitions of one input within one run. Select one input or split series by Input identity." }
            }
            Toggle {
                label: "Show mean ± 1 sample SD",
                value: plot.show_spread,
                on_change: move |show_spread| on_change.call(PlotOptions { show_spread, ..spread.clone() }),
            }
            Toggle {
                label: "Show min–max whiskers",
                value: plot.show_range,
                on_change: move |show_range| on_change.call(PlotOptions { show_range, ..range.clone() }),
            }
        }
        details { class: "more-options",
            summary { "More plot options" }
            label { class: "field",
                "Y minimum"
                input {
                    aria_label: "Y minimum",
                    r#type: "number",
                    step: "any",
                    placeholder: "Auto",
                    value: plot.y_min.map(| v | v.to_string())
                            .unwrap_or_default(),
                    onchange: move |e| {
                        on_change
                            .call(PlotOptions {
                                y_min: e.value().parse().ok(),
                                ..min.clone()
                            })
                    },
                }
            }
            label { class: "field",
                "Y maximum"
                input {
                    aria_label: "Y maximum",
                    r#type: "number",
                    step: "any",
                    placeholder: "Auto",
                    value: plot.y_max.map(|v| v.to_string()).unwrap_or_default(),
                    onchange: move |e| {
                        on_change
                            .call(PlotOptions {
                                y_max: e.value().parse().ok(),
                                ..max.clone()
                            })
                    },
                }
            }
            Toggle {
                label: "Show reference lines",
                value: plot
                        .show_reference_lines,
                on_change: move |v| {
                    on_change
                        .call(PlotOptions {
                            show_reference_lines: v,
                            ..lines.clone()
                        })
                },
            }
        }
    }
}
