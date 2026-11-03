use crate::api::Metadata;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use yosys_bench_ui_types::{analysis::*, experiment::*, metrics::*, store::*};
pub fn encoded(value: &impl Serialize) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
pub fn decoded<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_str(value).ok()
}
pub fn short(value: &str) -> String {
    value.chars().take(10).collect()
}
pub fn label(value: &str) -> String {
    value
        .split('.')
        .filter(|s| *s != "measurements")
        .map(|s| s.replace('_', " "))
        .collect::<Vec<_>>()
        .join(" / ")
}
pub fn number(value: Option<f64>) -> String {
    match value {
        None => "Unavailable".into(),
        Some(0.) => "0".into(),
        Some(v) if v.abs() < 0.001 || v.abs() >= 1e7 => format!("{v:.3e}"),
        Some(v) => format!("{v:.4}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .into(),
    }
}
pub fn version(v: Version) -> String {
    format!("{}.{}", v.major(), v.minor())
}
pub fn title(op: &Operation) -> &'static str {
    match op {
        Operation::History => "Performance by commit",
        Operation::Changes { .. } => "Changes",
        Operation::Scaling { .. } => "Scaling",
        Operation::Variation { .. } => "Variation",
        Operation::Coverage => "Coverage",
    }
}
pub fn reduction(r: Reduction) -> &'static str {
    match r {
        Reduction::Mean => "Mean",
        Reduction::Median => "Median",
        Reduction::Min => "Minimum",
        Reduction::Max => "Maximum",
        Reduction::Sum => "Sum",
        Reduction::Count => "Count",
        Reduction::StdDev => "Sample standard deviation",
        Reduction::P90 => "90th percentile",
        Reduction::P95 => "95th percentile",
        Reduction::GeometricMean => "Geometric mean",
    }
}
pub fn metric<'a>(meta: &'a Metadata, plan: &AnalysisPlan) -> Option<&'a MetricSpec> {
    meta.description
        .metrics
        .iter()
        .find(|m| Some(&m.path) == plan.metric.as_ref())
}
pub fn initial(meta: &Metadata) -> AnalysisPlan {
    let d = &meta.description;
    let mut plan = AnalysisPlan::history(
        ResultSelection::new(d.experiment.id),
        d.default_metric.clone().unwrap_or_default(),
    );
    plan.selection.version = d.versions.last().map(|v| v.version);
    plan.series = vec![Dimension::Input];
    if let Some(m) = metric(meta, &plan) {
        plan.repetitions = m.default_repetitions;
        plan.inputs = m.default_inputs;
    }
    plan
}

pub fn line_view(plan: &AnalysisPlan) -> &'static str {
    if plan.repetitions != Reduction::Mean || plan.inputs != Reduction::Mean {
        "custom"
    } else if plan.series == [Dimension::Input] {
        "inputs"
    } else if plan.series.is_empty() {
        "suite"
    } else {
        "custom"
    }
}

pub fn custom_settings(plan: &AnalysisPlan) -> bool {
    !matches!(plan.operation, Operation::History)
        || line_view(plan) == "custom"
        || plan.repetitions != Reduction::Mean
        || plan.inputs != Reduction::Mean
        || plan.coverage != CoveragePolicy::Strict
        || plan.normalization.is_some()
        || plan.cohort.is_some()
        || !plan.filters.is_empty()
        || !plan.display_filters.is_empty()
        || !plan.selection.run_filters.is_empty()
        || plan.selection.sources != VersionSources::CompatibleSupersets
        || plan.selection.partitions != PartitionSelection::Automatic
        || plan.selection.runs != RunSelection::LatestCompleted
        || plan.runs.is_some()
}

pub fn toggle_commit(meta: &Metadata, selected: &[String], id: &str) -> Vec<String> {
    let mut ids: BTreeSet<_> = selected.iter().map(String::as_str).collect();
    if !ids.insert(id) {
        ids.remove(id);
    }
    meta.revisions
        .iter()
        .filter(|r| ids.contains(r.id.as_str()))
        .map(|r| r.id.clone())
        .collect()
}

pub fn custom_commit_order(meta: &Metadata, query: &RevisionQuery) -> bool {
    if let RevisionQuery::Only { ids } = query {
        let recorded: Vec<_> = meta
            .revisions
            .iter()
            .filter(|r| ids.contains(&r.id))
            .map(|r| r.id.clone())
            .collect();
        &recorded != ids
    } else {
        false
    }
}

pub fn view_summary(plan: &AnalysisPlan) -> String {
    let inputs = match &plan.selection.inputs {
        InputSelection::All => "Whole suite".into(),
        InputSelection::Ids { ids } => format!("{} selected inputs", ids.len()),
        InputSelection::Matching { .. } => "Filtered inputs".into(),
    };
    let commits = match &plan.selection.revisions {
        RevisionQuery::All => "All commits".into(),
        RevisionQuery::Range { .. } => "Commit range".into(),
        RevisionQuery::Only { ids } => format!("{} selected commits", ids.len()),
    };
    let display = if !matches!(plan.operation, Operation::History) {
        title(&plan.operation)
    } else {
        match line_view(plan) {
            "inputs" => "One line per input",
            "suite" => "Suite average",
            _ => "Custom view",
        }
    };
    format!("{inputs} · {display} · {commits}")
}
#[cfg(test)]
pub fn switch_mode(plan: &mut AnalysisPlan, value: &str, meta: &Metadata) -> Option<String> {
    plan.operation = match value {
        "changes" => Operation::Changes {
            comparison: Comparison::AdjacentSelected,
            threshold: Threshold {
                value: 5.,
                kind: ChangeKind::Percent,
            },
            ranking: Ranking::Percent,
        },
        "scaling" => Operation::Scaling {
            x: meta
                .description
                .fields
                .iter()
                .find(|f| f.kind == FieldKind::Number)
                .or(meta.description.fields.first())
                .map(|f| f.path.clone())
                .unwrap_or_default(),
            reduce_remaining: false,
        },
        "variation" => Operation::Variation {
            population: if metric(meta, plan).is_some_and(|m| {
                matches!(
                    m.source,
                    MetricSource::Formula {
                        stage: FormulaStage::Input,
                        ..
                    }
                )
            }) {
                Population::AcrossInputs
            } else {
                Population::WithinRun
            },
        },
        "coverage" => Operation::Coverage,
        _ => Operation::History,
    };
    plan.paging.offset = 0;
    if !matches!(plan.operation, Operation::Coverage) && plan.metric.is_none() {
        plan.metric = meta.description.default_metric.clone();
        if let Some(m) = metric(meta, plan) {
            plan.repetitions = m.default_repetitions;
            plan.inputs = m.default_inputs;
        }
    }
    let mut cleared = Vec::new();
    if !matches!(
        plan.operation,
        Operation::History | Operation::Scaling { .. }
    ) && plan.normalization.take().is_some()
    {
        cleared.push("normalization");
    }
    if matches!(plan.operation, Operation::Coverage) {
        if !plan.filters.is_empty() {
            plan.filters.clear();
            cleared.push("calculation filters");
        }
        if !plan.display_filters.is_empty() {
            plan.display_filters.clear();
            cleared.push("display filters");
        }
        if plan.cohort.take().is_some() {
            cleared.push("the change cohort");
        }
    }
    let signed = metric(meta, plan).is_some_and(|m| m.domain == Domain::Finite);
    if let Operation::Changes { threshold, .. } = &mut plan.operation
        && signed
    {
        threshold.kind = ChangeKind::Absolute;
    }
    (!cleared.is_empty()).then(|| {
        format!(
            "{} cleared for {}.",
            cleared.join(", "),
            title(&plan.operation)
        )
    })
}
pub fn eligible_runs<'a>(meta: &'a Metadata, plan: &AnalysisPlan) -> Vec<&'a RunSummary> {
    meta.runs
        .iter()
        .filter(|r| {
            r.purpose == Purpose::Measurement
                && run_matches(r, &plan.selection.run_filters)
                && plan
                    .selection
                    .version
                    .is_none_or(|v| match plan.selection.sources {
                        VersionSources::Exact => r.version == v,
                        VersionSources::CompatibleSupersets => {
                            r.version.major() == v.major() && r.version.minor() >= v.minor()
                        }
                    })
        })
        .collect()
}
pub fn partitions(meta: &Metadata, plan: &AnalysisPlan) -> Vec<(String, String)> {
    let mut seen = BTreeSet::new();
    eligible_runs(meta, plan)
        .into_iter()
        .filter(|run| match &plan.selection.runs {
            RunSelection::Explicit { ids } => ids.contains(&run.id),
            _ => true,
        })
        .filter(|r| seen.insert(r.partition.clone()))
        .enumerate()
        .map(|(i, r)| {
            (
                r.partition.clone(),
                format!(
                    "{} / {} · configuration {}",
                    r.environment.machine,
                    r.environment.os,
                    i + 1,
                ),
            )
        })
        .collect()
}
pub fn ready(meta: &Metadata, plan: &AnalysisPlan) -> Option<String> {
    if meta.revision_order_required
        && !matches!(plan.selection.revisions, RevisionQuery::Only { .. })
        && !matches!(
            plan.operation,
            Operation::Changes {
                comparison: Comparison::Pair { .. },
                ..
            }
        )
    {
        return Some(
            "Choose commits to compare. Their order can be adjusted under Custom commit order."
                .into(),
        );
    }
    if let Some(reason) = &meta.description.unsupported {
        return Some(format!("Stored procedure unavailable: {reason}"));
    }
    if !matches!(plan.operation, Operation::Coverage) && plan.metric.is_none() {
        return Some("Select a metric.".into());
    }
    if matches!(&plan.selection.inputs, InputSelection::Ids { ids } if ids.is_empty()) {
        return Some("Choose at least one input.".into());
    }
    if matches!(&plan.selection.revisions, RevisionQuery::Only { ids } if ids.is_empty()) {
        return Some("Choose at least one commit.".into());
    }
    if partitions(meta, plan).len() > 1
        && matches!(plan.selection.partitions, PartitionSelection::Automatic)
    {
        return Some("Choose a measurement environment in the chart settings. Results from different machines or configurations are kept separate.".into());
    }
    if !matches!(plan.selection.runs, RunSelection::LatestCompleted)
        && plan.runs.is_none()
        && !matches!(
            plan.operation,
            Operation::Coverage | Operation::Variation { .. }
        )
    {
        return Some("This run selection needs a combination policy. Reset analysis settings to use the latest completed measurements.".into());
    }
    None
}
pub fn input_selected(selection: &InputSelection, input: &InputDescription) -> bool {
    match selection {
        InputSelection::All => true,
        InputSelection::Ids { ids } => ids.contains(&input.id),
        InputSelection::Matching { excluded, .. } => !excluded.contains(&input.id),
    }
}
pub fn toggle_input(selection: &mut InputSelection, id: InputId) {
    match selection {
        InputSelection::All => {
            *selection = InputSelection::Matching {
                filters: vec![],
                search: String::new(),
                excluded: vec![id],
            };
        }
        InputSelection::Ids { ids } => {
            if ids.contains(&id) {
                ids.retain(|i| *i != id)
            } else {
                ids.push(id)
            }
        }
        InputSelection::Matching { excluded, .. } => {
            if excluded.contains(&id) {
                excluded.retain(|i| *i != id)
            } else {
                excluded.push(id)
            }
        }
    }
}
pub fn filter_default(field: &InputField) -> Filter {
    Filter {
        field: field.path.clone(),
        operator: FilterOperator::Equal,
        value: match field.kind {
            FieldKind::Number => json!(0),
            FieldKind::Text => json!(""),
            FieldKind::Boolean => json!(true),
        },
    }
}
pub fn recipe(plan: &AnalysisPlan) -> String {
    let inputs = match &plan.selection.inputs {
        InputSelection::All => "All inputs".into(),
        InputSelection::Ids { ids } => format!("{} selected inputs", ids.len()),
        InputSelection::Matching {
            filters,
            search,
            excluded,
        } => {
            format!(
                "Matching inputs · {} filters{} · {} exclusions",
                filters.len(),
                if search.is_empty() {
                    String::new()
                } else {
                    format!(" · search: {search}")
                },
                excluded.len(),
            )
        }
    };
    let coverage = match plan.coverage {
        CoveragePolicy::Strict => "Full input coverage",
        CoveragePolicy::Common => "Fixed common inputs",
        CoveragePolicy::Available => "Available inputs",
    };
    let runs = match &plan.selection.runs {
        RunSelection::LatestCompleted => "Latest completed work items".into(),
        RunSelection::All => "All matching runs".into(),
        RunSelection::Explicit { ids } => format!("{} selected runs", ids.len()),
    };
    let treatment = match plan.runs {
        None => String::new(),
        Some(RunTreatment::Separate) => " · separate runs".into(),
        Some(RunTreatment::Pool) => " · pooled repetitions".into(),
        Some(RunTreatment::Summaries { reduction: r }) => {
            format!(" · {} of run summaries", reduction(r))
        }
    };
    let normalized = plan
        .normalization
        .as_ref()
        .map(|n| format!(" · {:?} vs {}", n.kind, short(&n.revision)))
        .unwrap_or_default();
    format!(
        "{inputs} · {coverage} · {runs}{treatment} · {} of repetitions → {} of inputs{normalized} · {} calculation filters",
        reduction(plan.repetitions),
        reduction(plan.inputs),
        plan.filters.len(),
    )
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AxisScale {
    #[default]
    Linear,
    Log,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlotOptions {
    pub y_scale: AxisScale,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    pub show_reference_lines: bool,
    pub show_spread: bool,
    #[serde(default)]
    pub show_range: bool,
}
impl Default for PlotOptions {
    fn default() -> Self {
        Self {
            y_scale: AxisScale::Linear,
            y_min: None,
            y_max: None,
            show_reference_lines: true,
            show_spread: true,
            show_range: true,
        }
    }
}
impl PlotOptions {
    pub fn bounds_error(&self) -> Option<&'static str> {
        if self
            .y_min
            .into_iter()
            .chain(self.y_max)
            .any(|v| !v.is_finite())
        {
            Some("Bounds must be finite.")
        } else if self.y_scale == AxisScale::Log
            && self.y_min.into_iter().chain(self.y_max).any(|v| v <= 0.)
        {
            Some("Logarithmic bounds must be positive.")
        } else if self.y_min.zip(self.y_max).is_some_and(|(a, b)| a >= b) {
            Some("Minimum must be less than maximum.")
        } else {
            None
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSpec {
    pub id: String,
    pub x_label: String,
    pub y_label: String,
    pub x_type: String,
    pub x_values: Vec<Value>,
    pub x_labels: Vec<String>,
    pub barmode: String,
    pub reference_lines: Vec<ReferenceLine>,
    pub spread_population: Option<String>,
    pub series: Vec<ChartSeries>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceLine {
    pub value: f64,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSeries {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub points: Vec<ChartPoint>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartPoint {
    pub x: Value,
    pub y: Option<f64>,
    pub spread: Option<Spread>,
    pub row: Option<String>,
    pub hover: String,
}
pub fn chart(result: &AnalysisResponse, meta: &Metadata) -> ChartSpec {
    let plan = &result.snapshot.plan;
    let name = metric(meta, plan)
        .map(|m| label(&m.label))
        .unwrap_or_else(|| "Coverage".into());
    let unit = result.unit.clone().unwrap_or_default();
    let mut chart = ChartSpec {
        id: result.id.clone(),
        x_label: "Commit".into(),
        y_label: format!("{name} ({unit})"),
        x_type: "category".into(),
        x_values: result.revisions.iter().map(|r| json!(r)).collect(),
        x_labels: result.revisions.iter().map(|r| short(r)).collect(),
        barmode: "group".into(),
        spread_population: match &plan.operation {
            Operation::Variation { population } => {
                Some(population_label(*population).to_lowercase())
            }
            Operation::History | Operation::Scaling { .. } => {
                Some("repetitions of one input within one run".into())
            }
            _ => None,
        },
        reference_lines: result
            .reference
            .map(|value| ReferenceLine {
                value,
                label: if value == 1. {
                    "Equal to baseline".into()
                } else {
                    "Reference".into()
                },
            })
            .into_iter()
            .collect(),
        series: vec![],
    };
    let configurations = partitions(meta, plan)
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let series_label = |name: &str, partition: &str| {
        if result.partitions.len() > 1 {
            format!(
                "{} · {}",
                name,
                configurations
                    .get(partition)
                    .map(String::as_str)
                    .unwrap_or(partition),
            )
        } else {
            name.into()
        }
    };
    let input_name = |id: InputId| {
        result
            .inputs
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.label.clone())
            .unwrap_or_else(|| format!("Input {id}"))
    };
    if plan.inputs == Reduction::StdDev
        && matches!(
            plan.operation,
            Operation::History | Operation::Scaling { .. }
        )
    {
        chart.y_label = format!("Input sample SD: {name} ({unit})");
    }
    match &plan.operation {
        Operation::History | Operation::Scaling { .. } => {
            for s in &result.series {
                chart.series.push(ChartSeries {
                    id: s.id.clone(),
                    label: series_label(&s.label, &s.partition),
                    kind: "scatter".into(),
                    points: s
                        .points
                        .iter()
                        .map(|p| ChartPoint {
                            x: p.x.clone(),
                            y: p.value,
                            spread: p.repetition_spread.clone(),
                            row: Some(p.id.clone()),
                            hover: {
                                let revision = p
                                    .details
                                    .get("revision")
                                    .map(String::as_str)
                                    .unwrap_or_default();
                                let message = meta
                                    .revisions
                                    .iter()
                                    .find(|r| r.id == revision)
                                    .and_then(|r| r.message.lines().next())
                                    .unwrap_or_default();
                                let x = if let Operation::Scaling { x, .. } = &plan.operation {
                                    format!("{}: {}\n", label(x), value_text(&p.x))
                                } else {
                                    String::new()
                                };
                                format!(
                                    "{x}{} {}\n{} inputs · {} measurements",
                                    short(revision),
                                    message,
                                    p.coverage.included_inputs,
                                    p.coverage.valid_repetitions
                                )
                            },
                        })
                        .collect(),
                });
            }
            if let Operation::Scaling { x, .. } = &plan.operation {
                chart.x_label = meta
                    .description
                    .fields
                    .iter()
                    .find(|f| f.path == *x)
                    .map(|f| f.label.clone())
                    .unwrap_or_else(|| label(x));
                chart.x_type = if result
                    .series
                    .iter()
                    .flat_map(|s| &s.points)
                    .all(|p| p.x.is_number())
                {
                    "linear"
                } else {
                    "category"
                }
                .into();
                chart.x_values = chart
                    .series
                    .iter()
                    .flat_map(|s| s.points.iter().map(|p| p.x.clone()))
                    .collect();
                chart.x_labels = chart.x_values.iter().map(value_text).collect();
            }
        }
        Operation::Changes { ranking, .. } => {
            chart.x_label = "Inputs ranked by change".into();
            chart.x_values = result.changes.items.iter().map(|r| json!(r.id)).collect();
            chart.x_labels = result
                .changes
                .items
                .iter()
                .map(|r| input_name(r.input))
                .collect();
            chart.y_label = match ranking {
                Ranking::Percent => "Change (%)".into(),
                Ranking::Absolute => format!("Change ({unit})"),
                Ranking::Contribution => format!("Contribution to suite change ({unit})"),
            };
            chart.reference_lines = vec![ReferenceLine {
                value: 0.,
                label: "No change".into(),
            }];
            chart.series.push(ChartSeries {
                id: "changes".into(),
                label: "Per-input change".into(),
                kind: "bar".into(),
                points: result
                    .changes
                    .items
                    .iter()
                    .map(|r| ChartPoint {
                        x: json!(r.id),
                        y: match ranking {
                            Ranking::Absolute => r.absolute,
                            Ranking::Percent => r.percent,
                            Ranking::Contribution => r.contribution,
                        },
                        spread: None,
                        row: Some(r.id.clone()),
                        hover: format!(
                            "{} · {} → {} · {:?}",
                            input_name(r.input),
                            short(&r.baseline),
                            short(&r.current),
                            r.class,
                        ),
                    })
                    .collect(),
            });
        }
        Operation::Variation { population } => {
            chart.y_label = format!("Mean ({unit})");
            let mut series = BTreeMap::<String, ChartSeries>::new();
            for d in &result.distributions.items {
                let key = encoded(&(d.input, &d.partition, d.run, &d.dimensions));
                let name = d.input.map(&input_name).unwrap_or_else(|| {
                    if d.dimensions.is_empty() {
                        "Input estimates".into()
                    } else {
                        d.dimensions
                            .iter()
                            .map(|(k, v)| format!("{}={}", label(k), value_text(v)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                });
                let s = series.entry(key.clone()).or_insert_with(|| ChartSeries {
                    id: key,
                    label: format!(
                        "{}{}",
                        series_label(&name, &d.partition),
                        d.run.map(|r| format!(" · run {r}")).unwrap_or_default(),
                    ),
                    kind: "scatter".into(),
                    points: vec![],
                });
                s.points.push(ChartPoint {
                    x: json!(d.revision),
                    y: d.mean,
                    spread: d
                        .mean
                        .zip(d.min)
                        .zip(d.max)
                        .map(|((mean, min), max)| Spread {
                            count: d.count,
                            mean,
                            min,
                            max,
                            standard_deviation: d.standard_deviation,
                        }),
                    row: Some(d.id.clone()),
                    hover: format!("{} · n={} · {:?}", short(&d.revision), d.count, population,),
                });
            }
            chart.series = series
                .into_values()
                .map(|mut series| {
                    let points = series
                        .points
                        .into_iter()
                        .map(|p| (value_text(&p.x), p))
                        .collect::<BTreeMap<_, _>>();
                    series.points = result
                        .revisions
                        .iter()
                        .map(|r| {
                            points.get(r).cloned().unwrap_or_else(|| ChartPoint {
                                x: json!(r),
                                y: None,
                                spread: None,
                                row: None,
                                hover: "No estimate at this revision".into(),
                            })
                        })
                        .collect();
                    series
                })
                .collect();
        }
        Operation::Coverage => {
            chart.y_label = "Selected input/environment slots".into();
            chart.reference_lines.clear();
            chart.barmode = "stack".into();
            let mut slots = BTreeMap::<(String, String, InputId), BTreeSet<String>>::new();
            for c in &result.coverage {
                slots
                    .entry((c.revision.clone(), c.partition.clone(), c.input))
                    .or_default()
                    .insert(coverage_status(c).into());
            }
            let mut counts = BTreeMap::<String, BTreeMap<String, usize>>::new();
            for ((revision, _, _), states) in slots {
                let state = if states.len() == 1 {
                    states.into_iter().next().unwrap()
                } else {
                    "Mixed".into()
                };
                *counts
                    .entry(state)
                    .or_default()
                    .entry(revision)
                    .or_default() += 1;
            }
            chart.series = counts
                .into_iter()
                .map(|(state, counts)| ChartSeries {
                    id: state.clone(),
                    label: state.clone(),
                    kind: "bar".into(),
                    points: result
                        .revisions
                        .iter()
                        .map(|r| ChartPoint {
                            x: json!(r),
                            y: Some(*counts.get(r).unwrap_or(&0) as f64),
                            spread: None,
                            row: None,
                            hover: format!(
                                "{} · {state} · inspect individual rows below",
                                short(r),
                            ),
                        })
                        .collect(),
                })
                .collect();
        }
    }
    chart
}
pub fn value_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
pub fn coverage_status(cell: &CoverageCell) -> &'static str {
    match cell.state {
        None => "Unmeasured",
        Some(State::BuildFailed) => "Build failed",
        Some(State::Planned | State::Running) => "Unfinished",
        Some(State::Interrupted) => "Interrupted",
        Some(State::Complete) => {
            let o = &cell.outcomes;
            if o.success > 0 && (o.failure > 0 || o.unsupported > 0) {
                "Mixed"
            } else if o.failure > 0 {
                "Failed"
            } else if o.unsupported > 0 {
                "Unsupported"
            } else if o.success > 0 {
                "Measured"
            } else {
                "No observations"
            }
        }
    }
}
pub fn change_class(value: ChangeClass) -> &'static str {
    match value {
        ChangeClass::Improved => "Improved",
        ChangeClass::Worsened => "Worsened",
        ChangeClass::WithinThreshold => "Within threshold",
        ChangeClass::Unclassified => "Unclassified",
        ChangeClass::Unavailable => "Unavailable",
    }
}
pub fn timestamp(value: i64) -> String {
    chrono::DateTime::from_timestamp_millis(value)
        .map(|date| date.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "Unknown".into())
}
fn run_matches(run: &RunSummary, filters: &[Filter]) -> bool {
    filters.iter().all(|filter| {
        let value = match filter.field.as_str() {
            "environment.machine" => Some(json!(run.environment.machine)),
            "environment.os" => Some(json!(run.environment.os)),
            "environment.arch" => Some(json!(run.environment.arch)),
            field if field.starts_with("environment.") => run
                .environment
                .attributes
                .get(&field[12..])
                .map(|v| json!(v)),
            field if field.starts_with("option:") => run.options.pointer(&field[7..]).cloned(),
            _ => None,
        };
        filter.matches(value.as_ref())
    })
}
pub fn configuration_name(meta: &Metadata, plan: &AnalysisPlan, partition: &str) -> String {
    partitions(meta, plan)
        .into_iter()
        .find(|(id, _)| id == partition)
        .map(|(_, name)| name)
        .unwrap_or_else(|| "Selected configuration".into())
}
pub fn population_label(population: Population) -> &'static str {
    match population {
        Population::WithinRun => "Repetitions within one run",
        Population::BetweenRuns => "Per-run estimates",
        Population::AcrossInputs => "Input estimates (workload spread)",
    }
}

/// Commit metadata for the selected point, including explicit change endpoints.
pub fn row_revisions(result: &AnalysisResponse, row: &str) -> Vec<String> {
    if let Some(change) = result.changes.items.iter().find(|c| c.id == row) {
        return vec![change.baseline.clone(), change.current.clone()];
    }
    if let Some(distribution) = result.distributions.items.iter().find(|d| d.id == row) {
        return vec![distribution.revision.clone()];
    }
    if let Some(cell) = result.coverage.iter().find(|c| c.id == row) {
        return vec![cell.revision.clone()];
    }
    result
        .series
        .iter()
        .flat_map(|s| &s.points)
        .find(|p| p.id == row)
        .and_then(|p| p.details.get("revision"))
        .cloned()
        .into_iter()
        .collect()
}

pub fn commit_time(value: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(value)
        .format("%Y-%m-%d %H:%M UTC")
        .to_string()
}
