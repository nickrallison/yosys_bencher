use crate::{api::Metadata, inputs::InputPicker, model::*};
use dioxus::prelude::*;
use yosys_bencher_ui_types::{analysis::*, api::ApiError, metrics::*, store::*};
#[derive(Clone)]
pub enum Edit {
    Metric(String),
    Version(Version),
    Sources(VersionSources),
    Inputs(InputSelection),
    Revisions(RevisionQuery),
    Partitions(PartitionSelection),
    View(String),
    ResetSettings,
    Coverage(CoveragePolicy),
    Normalization(Option<Normalization>),
}
pub fn apply(plan: &mut AnalysisPlan, edit: Edit, meta: &Metadata) -> Option<String> {
    plan.paging.offset = 0;
    match edit {
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
        Edit::View(value) => {
            plan.series = if value == "inputs" {
                vec![Dimension::Input]
            } else {
                vec![]
            };
            plan.inputs = Reduction::Mean;
            plan.repetitions = Reduction::Mean;
        }
        Edit::ResetSettings => {
            let individual = !plan.series.is_empty();
            let path = plan
                .metric
                .clone()
                .or_else(|| meta.description.default_metric.clone())
                .unwrap_or_default();
            let mut selection = plan.selection.clone();
            selection.sources = VersionSources::CompatibleSupersets;
            selection.partitions = PartitionSelection::Automatic;
            selection.runs = RunSelection::LatestCompleted;
            selection.run_filters.clear();
            *plan = AnalysisPlan::history(selection, path);
            if individual {
                plan.series = vec![Dimension::Input];
            }
            return Some(
                "Analysis settings reset. Metric, inputs, commits and workload version were kept."
                    .into(),
            );
        }
        Edit::Coverage(v) => plan.coverage = v,
        Edit::Normalization(v) => plan.normalization = v,
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
    let error_field = error.as_ref().and_then(|e| e.field.clone());
    rsx! {
        aside { class: "analysis-sidebar", aria_label: "Chart controls",
            section { class: "editor-section", id: "metric-controls",
                h2 { "Metric" }
                crate::metric_picker::MetricPicker {
                    metrics: meta.description.metrics.clone(),
                    value: plan.metric.clone(),
                    allow_none: matches!(plan.operation, Operation::Coverage),
                    on_change: move |v| on_edit.call(Edit::Metric(v)),
                }
                if let Some(m) = &m {
                    p { class: "muted", "{m.description}" }
                    if m.direction == Direction::Lower { p { class: "muted", "Lower is better." } }
                    if m.direction == Direction::Higher { p { class: "muted", "Higher is better." } }
                }
            }
            section { class: "editor-section", id: "input-controls",
                h2 { "Inputs" }
                InputPicker { meta: meta.clone(), selection: plan.selection.clone(), on_edit }
            }
            section { class: "editor-section", id: "commit-controls",
                h2 { "Commits" }
                CommitPicker {
                    meta: meta.clone(), value: plan.selection.revisions.clone(),
                    on_change: move |v| on_edit.call(Edit::Revisions(v)),
                }
                details { class: "more-options",
                    summary { "Custom commit order" }
                    CommitPicker { custom_order: true, meta: meta.clone(), value: plan.selection.revisions.clone(), on_change: move |value| on_edit.call(Edit::Revisions(value)) }
                }
            }
            if matches!(plan.operation, Operation::History) {
                section { class: "editor-section", id: "display-controls",
                    h2 { "Display" }
                    DisplayControls { plan: plan.clone(), on_edit }
                }
            }
            // Shelved controls and their restoration notes live in docs/cut-for-simplicity.md.
            HistorySettings { meta: meta.clone(), plan: plan.clone(), on_edit }
            section { class: "editor-section", id: "plot-controls",
                h2 { "Axes and reference lines" }
                PlotControls { meta: meta.clone(), plan: plan.clone(), plot, on_change: on_plot }
            }
            div { class: "apply-controls",
                if let Some(field) = error_field {
                    p { class: "error",
                        if field == "metric" { "Choose a metric above." }
                        else if field.starts_with("selection.inputs") { "Check the selected inputs above." }
                        else if field.starts_with("selection.revisions") { "Check the selected commits above." }
                        else { "Check {field} in the chart settings." }
                    }
                }
                if custom_settings(&plan) {
                    button {
                        r#type: "button", title: "Keeps your metric, selected inputs, commits and workload version.",
                        onclick: move |_| on_edit.call(Edit::ResetSettings), "Reset analysis settings"
                    }
                }
                button { class: "primary", onclick: move |_| on_apply.call(()), "Update chart" }
                if pending { p { class: "muted", role: "status", "Unapplied changes" } }
            }
        }
    }
}

#[component]
fn DisplayControls(plan: AnalysisPlan, on_edit: EventHandler<Edit>) -> Element {
    let view = line_view(&plan);
    let mut options = vec![
        ("inputs".into(), "One line per input".into()),
        ("suite".into(), "Suite average".into()),
    ];
    if view == "custom" {
        options.push(("custom".into(), "Custom view".into()));
    }
    rsx! {
        Choice {
            label: "Lines", value: view, options,
            on_change: move |value: String| { if value != "custom" { on_edit.call(Edit::View(value)); } },
        }
        p { class: "muted",
            if view == "suite" { "One line averages the selected inputs. Choose one line per input to see repeat variability." }
            else if view == "custom" { "This chart uses custom averaging or grouping. Choose a line option above to use means again." }
            else if custom_settings(&plan) { "The settings below change how these values are calculated." }
            else { "Each point is the mean of repeated measurements of that input." }
        }

    }
}

#[component]
fn HistorySettings(meta: Metadata, plan: AnalysisPlan, on_edit: EventHandler<Edit>) -> Element {
    let configurations = partitions(&meta, &plan);
    let selected = match &plan.selection.partitions {
        PartitionSelection::Only { ids } => ids.first().cloned().unwrap_or_default(),
        _ => String::new(),
    };
    rsx! {
        // Other analysis views are archived in docs/cut-for-simplicity.md; history is the only UI view.
        section { class: "editor-section", id: "version-controls",
            h2 { "Data versions" }
            Choice {
                label: "Workload version",
                value: plan.selection.version.map(version).unwrap_or_default(),
                options: meta.description.versions.iter().map(|v| (
                    version(v.version), format!("v{} · {} inputs", version(v.version), v.input_count),
                )).collect(),
                on_change: {
                    let versions = meta.description.versions.clone();
                    move |value: String| { if let Some(v) = versions.iter().find(|v| version(v.version) == value) { on_edit.call(Edit::Version(v.version)); } }
                },
            }
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

            p { class: "muted", "Controls which recorded workload definitions can supply measurements. The default uses compatible versions." }
        }
        // LatestCompleted is the default; explicit/all-run controls are archived.
        // Render an environment choice only when Automatic would reject ambiguous results.
        if configurations.len() > 1 {
            section { class: "editor-section", id: "environment-controls",
                h2 { "Environment" }
                Choice {
                    label: "Measurement environment", value: selected,
                    options: std::iter::once((String::new(), "Choose an environment".into())).chain(configurations).collect(),
                    on_change: move |id: String| on_edit.call(Edit::Partitions(
                        if id.is_empty() { PartitionSelection::Automatic }
                        else { PartitionSelection::Only { ids: vec![id] } }
                    )),
                }
                p { class: "muted", "Results contain multiple machines or configurations. Choose one so their timings are not mixed." }
            }
        }
        section { class: "editor-section", id: "operation-controls",
            h2 { "Compare to a commit" }
            HistoryControls { meta: meta.clone(), plan: plan.clone(), on_edit }
        }
        // Generic reducers, series dimensions, filters and cohorts are archived in docs/cut-for-simplicity.md.
        // A focused slow-input selection workflow is proposed there before exposing more controls.
        section { class: "editor-section", id: "coverage-controls",
            h2 { "Missing results" }
            Choice {
                label: "When an input has no result",
                value: encoded(&plan.coverage),
                options: vec![
                    (encoded(&CoveragePolicy::Strict), "Leave a gap".into()),
                    (encoded(&CoveragePolicy::Common), "Use inputs measured at every commit".into()),
                    (encoded(&CoveragePolicy::Available), "Use available inputs at each commit".into()),
                ],
                on_change: move |v: String| {
                    if let Some(v) = decoded(&v) { on_edit.call(Edit::Coverage(v)); }
                },
            }
            p { class: "muted",
                match plan.coverage {
                    CoveragePolicy::Strict => "A suite average needs every selected input. If one is missing, that commit has a gap.",
                    CoveragePolicy::Common => "Keep only inputs measured at every displayed commit. Every average uses that same set.",
                    CoveragePolicy::Available => "Average whatever inputs have results at each commit. The set can change, which can look like a speedup or slowdown.",
                }
            }
            p { class: "muted", "Missing measurements are never filled from another input. The shared-input option also excludes individual lines without results at every commit." }
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
    #[props(default)] custom_order: bool,
    on_change: EventHandler<RevisionQuery>,
) -> Element {
    let mut query = use_signal(String::new);
    let custom_selection = custom_commit_order(&meta, &value);
    let mode = match value {
        RevisionQuery::All => "all",
        RevisionQuery::Range { .. } => "range",
        RevisionQuery::Only { .. } => "only",
    };
    let (start, end) = match &value {
        RevisionQuery::Range { start, end } => (start.clone(), end.clone()),
        _ => (None, None),
    };
    let mut starts = vec![(String::new(), "First recorded commit".into())];
    starts.extend(revision_options(&meta));
    let mut ends = vec![(String::new(), "Last recorded commit".into())];
    ends.extend(revision_options(&meta));
    let search = query().to_lowercase();
    rsx! {
        Choice {
            label: "Commit selection", value: mode,
            options: vec![("all".into(), "All commits".into()), ("range".into(), "Commit range".into()), ("only".into(), "Choose commits".into())],
            on_change: move |value: String| {
                query.set(String::new());
                on_change.call(match value.as_str() {
                    "range" => RevisionQuery::Range { start: None, end: None },
                    "only" => RevisionQuery::Only { ids: vec![] },
                    _ => RevisionQuery::All,
                });
            },
        }
        if mode == "range" {
            Choice {
                label: "First commit", value: start.clone().unwrap_or_default(), options: starts,
                on_change: { let end = end.clone(); move |value: String| on_change.call(RevisionQuery::Range { start: (!value.is_empty()).then_some(value), end: end.clone() }) },
            }
            Choice {
                label: "Last commit", value: end.unwrap_or_default(), options: ends,
                on_change: move |value: String| on_change.call(RevisionQuery::Range { start: start.clone(), end: (!value.is_empty()).then_some(value) }),
            }
            p { class: "muted", "Includes both endpoints and the commits between them." }
        }
        if let RevisionQuery::Only { ids } = &value {
            div { class: "commit-picker",
                input { r#type: "search", aria_label: "Search commits", placeholder: "Search hash, message or author", value: query(), oninput: move |event| query.set(event.value()) }
                div { class: "picker-actions",
                    button { r#type: "button", onclick: { let ids = meta.revisions.iter().map(|r| r.id.clone()).collect::<Vec<_>>(); move |_| on_change.call(RevisionQuery::Only { ids: ids.clone() }) }, "Select all" }
                    button { r#type: "button", onclick: move |_| on_change.call(RevisionQuery::Only { ids: vec![] }), "Clear" }
                    span { class: "muted", "{ids.len()} selected" }
                }
                div { class: "picker-list commit-list",
                    for revision in meta.revisions.iter().filter(|r| format!("{} {} {}", r.id, r.message, r.author).to_lowercase().contains(&search)) {
                        label { class: "check-row", title: format!("{} · {}\n{}", revision.author, timestamp(revision.committed_at.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64), revision.message),
                            input {
                                r#type: "checkbox", aria_label: "Include commit {short(&revision.id)}", checked: ids.contains(&revision.id),
                                onchange: { let meta = meta.clone(); let ids = ids.clone(); let id = revision.id.clone(); move |_| {
                                    let next = if custom_order {
                                        let mut next = ids.clone();
                                        if next.contains(&id) { next.retain(|v| v != &id); } else { next.push(id.clone()); }
                                        next
                                    } else { toggle_commit(&meta, &ids, &id) };
                                    on_change.call(RevisionQuery::Only { ids: next });
                                } },
                            }
                            span { code { "{short(&revision.id)}" } small { "{revision.message.lines().next().unwrap_or_default()}" } }
                        }
                    }
                }
                p { class: "muted",
                    if custom_order { "Selection order determines the X axis." }
                    else if custom_selection { "Custom order is selected below." }
                    else { "Shown in recorded commit order." }
                }
            }
        }
    }
}
// The former multi-mode controls are archived in docs/cut-for-simplicity.md.
#[component]
fn HistoryControls(meta: Metadata, plan: AnalysisPlan, on_edit: EventHandler<Edit>) -> Element {
    let baseline = meta
        .revisions
        .first()
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
    rsx! {
        Toggle {
            label: "Show change relative to a commit", value: plan.normalization.is_some(), disabled: suite,
            on_change: move |enabled: bool| on_edit.call(Edit::Normalization(enabled.then(|| Normalization {
                revision: baseline.clone(), kind: ChangeKind::Ratio, runs: None,
            }))),
        }
        if let Some(n) = plan.normalization {
            RevisionField {
                label: "Baseline commit", value: n.revision.clone(), meta,
                on_change: { let n = n.clone(); move |revision| on_edit.call(Edit::Normalization(Some(Normalization { revision, ..n.clone() }))) },
            }
            Choice {
                label: "Show change as", value: encoded(&n.kind),
                options: vec![
                    (encoded(&ChangeKind::Ratio), "Ratio to baseline".into()),
                    (encoded(&ChangeKind::Percent), "Percent change from baseline".into()),
                    (encoded(&ChangeKind::Absolute), "Absolute difference".into()),
                ],
                on_change: move |value: String| {
                    if let Some(kind) = decoded(&value) {
                        on_edit.call(Edit::Normalization(Some(Normalization { kind, ..n.clone() })));
                    }
                },
            }
            p { class: "muted", "Compare each input with its own baseline before averaging inputs. The baseline can be outside the displayed commits." }
        }
        if suite { p { class: "muted", "This metric combines inputs before calculating its value, so comparison to a baseline is unavailable." } }
    }
}
#[component]
fn PlotControls(
    meta: Metadata,
    plan: AnalysisPlan,
    plot: PlotOptions,
    on_change: EventHandler<PlotOptions>,
) -> Element {
    let scale = plot.clone();
    let min = plot.clone();
    let max = plot.clone();
    let lines = plot.clone();
    let averaged_metric = metric(&meta, &plan).is_some_and(|m| {
        matches!(
            m.source,
            MetricSource::Formula {
                stage: FormulaStage::Input | FormulaStage::Suite,
                ..
            }
        )
    });
    let spread_disabled = line_view(&plan) != "inputs"
        || averaged_metric
        || plan.normalization.is_some()
        || matches!(plan.runs, Some(RunTreatment::Summaries { .. }));
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
        if matches!(plan.operation, Operation::History) {
            Toggle {
                label: "Standard deviation (±1 SD)", value: plot.show_spread && !spread_disabled, disabled: spread_disabled,
                on_change: { let plot = plot.clone(); move |show_spread| on_change.call(PlotOptions { show_spread, ..plot.clone() }) },
            }
            Toggle {
                label: "Range (min–max)", value: plot.show_range && !spread_disabled, disabled: spread_disabled,
                on_change: { let plot = plot.clone(); move |show_range| on_change.call(PlotOptions { show_range, ..plot.clone() }) },
            }
            if averaged_metric {
                p { class: "muted", "Bars are unavailable for metrics calculated from averages. Select a runtime metric to see repeat variability." }
            } else if plan.normalization.is_some() {
                p { class: "muted", "Bars are unavailable while comparing to a baseline commit. Turn off the comparison to see repeat variability." }
            } else if matches!(plan.runs, Some(RunTreatment::Summaries { .. })) {
                p { class: "muted", "Bars are unavailable for averages across runs. Use measurements from one run to see repeat variability." }
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
