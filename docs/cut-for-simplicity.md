# Cut for simplicity

The main workflow is metric → inputs → commits → history. Each input gets a line
showing its mean, optional min–max whiskers and ±1 sample SD across repetitions.
Suite average combines input estimates; it does not show repetition spread.
Axes and reference lines are the last settings in the sidebar. Remaining settings
are inline; there is no Advanced menu. Latest completed work items remain the
default without a Measurements card.

These controls are removed from the UI, not from the analysis engine or stored
data. Source snapshots below allow restoration. Live mount sites in
`webapp/src/controls.rs` point here. Restore features around specific user
questions, with a preview of what they select or change.

## What was cut

| Controls | What they did | Why shelved |
| --- | --- | --- |
| Other analysis views | Changes ranked commit comparisons; Scaling plotted against an input parameter; Variation compared distributions; Coverage counted result states. | History is the useful daily view. The others need focused designs. |
| Run selection and treatment | Selected explicit/all runs; kept them separate, averaged run summaries, or pooled repetitions. | Similar choices produced different sample populations. Use the latest completed work item for each input/commit. |
| Environment tools | Selected all configurations, inspected JSON and filtered run metadata. | Too much machinery for the usual case. A single-environment chooser remains only when multiple configurations exist, to prevent silently mixing machines or options. |
| Custom grouping and averages | Chose reducers for repetitions and inputs; grouped lines by input/run/environment/parameters/options. | Two reducers plus arbitrary dimensions obscured what a point meant. Keep “One line per input” and “Suite average”. |
| Calculation filters | Removed attempts by outcome, or input estimates by numeric value before normalization. | Changed numbers without an obvious account of what was removed. |
| Display filters | Hid calculated points/rows without changing estimates. | Looked like calculation filters while doing something different. |
| Fixed change cohort | Selected inputs that worsened/improved between two commits, then kept those identities throughout history. | Useful idea hidden behind cohort/threshold terminology. Needs a selection preview. |

## Missing results

“Coverage policy” is now “Missing results” in the sidebar:

- **Leave a gap:** a suite point needs every selected input.
- **Use inputs measured at every commit:** use one fixed shared set for the whole chart.
- **Use available inputs at each commit:** use whatever exists; the input set can change.

Example: A and B have all results; C is missing at one commit. The choices give
one gap, A+B throughout, or A+B+C except at that commit. The last option can look
like a speedup simply because a slow input disappeared. A missing individual
measurement is never filled from another input.

## Workshop: find the inputs worth investigating

Replace the generic forms with two explicit tasks:

1. **Slowest inputs at a commit.** Pick a metric and commit. Choose “Top 10” or
   “At or above the 95th percentile” (the slowest roughly 5% for runtime).
2. **Largest slowdowns between commits.** Pick a baseline and comparison commit,
   then choose “Top 10 slowdowns” or “Slower by at least 5%”. Show absolute and
   percent change so tiny baselines do not hide the practical cost.

Both should preview a table: input, baseline/comparison values where relevant,
mean, measurement count, and why it qualified. Show the cutoff, matched count,
and missing/invalid count. Rank the mean of repetitions for each input; keep
repetition variability in the chart and inspector. Include ties at the cutoff.
For “slowest”, runtime ranks descending; other metrics need a named direction
rather than assuming larger always means worse.

“Plot these inputs” should freeze their IDs, then show those same inputs across
all selected commits. Do not rerank each commit: that changes which workload
the chart represents. Keep the selection editable in Inputs, with a short note
of the commit and rule that produced it.

“95 percent worst” is ambiguous. The proposed label explicitly means values at
or above P95, not the slowest 95% of inputs. The existing P95 reducer computes
one percentile value; it does not select input IDs. This proposal is not exposed
as a working feature yet. Agree on the task before adding more controls.

## How investigation could fit into the existing graph

These are design options, not implemented controls. Keep history's X axis as
commits and its Y axis as the selected metric. Ranking selects or highlights
input lines; it does not replace runtime with a percentile or regression score.

| Option | How it works with history | Tradeoff |
| --- | --- | --- |
| Ranked list beside/below the graph | “Find inputs” opens a small table. Choose one commit for slowest inputs, or two for slowdowns. Clicking a row highlights its existing history line; “Show selected inputs” focuses the graph. | Best starting point: familiar table, graph keeps its meaning. Needs space for the list. |
| Commit markers on the graph | “Compare commits” enables two vertical markers. Move them to commits; a table below lists inputs that slowed down between them. Selecting rows highlights or focuses their full histories. | Makes the interval visible. More interaction work; must preserve ordinary point-click inspection. |
| Heatmap companion | Rows are inputs, columns are commits; color shows change relative to a baseline. Clicking a cell opens that input's history with repetition bars. | Helps with hundreds of inputs, but introduces a new visualization. Defer until line count makes it necessary. |

Recommended first version: a ranked list in the chart card, opened by **Find
inputs**. Default its comparison to the first and last displayed commits, and
show their hashes/messages. A single-commit mode ranks raw runtime instead.
Start with Top 10 and a minimum slowdown threshold; add percentile selection
only when users need it.

Example: choose commits A and B → see “input X: 1.0 s → 1.6 s, +0.6 s (+60%)” →
click X to highlight its line across the full history. The chart still shows
runtime, with markers at A and B. Choose several rows and use **Show selected
inputs** to update the existing input picker. Offer **Clear focus** to restore
the input selection from before focusing. Use a separate action for opening input details.

Prefer highlighting on click; hovering should only give a temporary preview.
Use a light stroke emphasis or a clear focused subset rather than making every
other line almost invisible. Sorting the table must not reorder commits or change
the Y axis. Switching to a metric that cannot be ranked should explain why.

Once applied, keep selection fixed even if the comparison commits or metric
change. Mark the ranking as out of date and offer **Recalculate**. Show the
selected-input count and selection rule so a hidden subset never looks like
the whole suite. Applying a percentile threshold independently at each commit
would select different inputs and should not be the default.

## Restoring code

These are original UI snapshots, not a second compiled implementation. Restore
only the needed component, its Edit variants/handlers and imports. Backend
operations, reducers, filters, cohorts, run policies and inspection remain.
`switch_mode` remains a test helper so archived backend operations stay covered.
Input-parameter filters in the primary input picker remain available.

<details>
<summary>Former Advanced layout and analysis selector</summary>

```rust
#[component]
fn AdvancedSidebar(
    meta: Metadata,
    plan: AnalysisPlan,
    plot: PlotOptions,
    on_edit: EventHandler<Edit>,
    on_plot: EventHandler<PlotOptions>,
) -> Element {
    let m = metric(&meta, &plan).cloned();
    let is_coverage = matches!(plan.operation, Operation::Coverage);
    rsx! {
        section { class: "editor-section",
            h2 { "Other analysis views" }
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

        }
        section { class: "editor-section",
            details { class: "more-options",
                summary { "Custom commit order" }
                CommitPicker { custom_order: true, meta: meta.clone(), value: plan.selection.revisions.clone(), on_change: move |value| on_edit.call(Edit::Revisions(value)) }
            }
        }
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
                h2 { "Custom grouping and averages" }
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
            h2 { "Axes and reference lines" }
            AdvancedPlotControls { plot, operation: plan.operation.clone(), on_change: on_plot }
        }
    }
}
```

</details>

<details>
<summary>Reduction selector</summary>

```rust
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
```

</details>

<details>
<summary>Run, analysis, grouping, filter and cohort controls</summary>

```rust
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
```

</details>

<details>
<summary>Original Edit actions and handlers</summary>

```rust
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
    View(String),
    ResetAdvanced,
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
        Edit::View(value) => {
            plan.series = if value == "inputs" {
                vec![Dimension::Input]
            } else {
                vec![]
            };
            plan.inputs = Reduction::Mean;
            plan.repetitions = Reduction::Mean;
        }
        Edit::ResetAdvanced => {
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
            return Some("Advanced analysis settings reset. Metric, inputs, commits and workload version were kept.".into());
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
```

</details>

<details>
<summary>Mode helpers</summary>

```rust
pub fn mode(op: &Operation) -> &'static str {
    match op {
        Operation::History => "history",
        Operation::Changes { .. } => "changes",
        Operation::Scaling { .. } => "scaling",
        Operation::Variation { .. } => "variation",
        Operation::Coverage => "coverage",
    }
}
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
```

</details>

