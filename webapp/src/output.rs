use crate::{api::Metadata, chart::PlotlyChart, model::*};
use dioxus::prelude::*;
use yosys_bench_ui_types::{
    analysis::*,
    metrics::{FormulaStage, MetricSource, Reduction},
    store::InputId,
};

#[component]
pub fn Pager(
    offset: usize,
    limit: usize,
    total: usize,
    on_change: EventHandler<Paging>,
) -> Element {
    rsx! {nav{class:"pagination",aria_label:"Result pages",
        span{class:"muted",if total==0{"No rows"}else{"{offset+1}–{(offset+limit).min(total)} of {total}"}}
        button{disabled:offset==0,onclick:move|_|on_change.call(Paging{offset:offset.saturating_sub(limit),limit}),"Previous"}
        button{disabled:offset+limit>=total,onclick:move|_|on_change.call(Paging{offset:offset+limit,limit}),"Next"}
    }}
}
#[component]
pub fn AnalysisOutput(
    meta: Metadata,
    result: Option<AnalysisResponse>,
    plot: PlotOptions,
    loading: bool,
    on_select: EventHandler<String>,
    on_page: EventHandler<Paging>,
) -> Element {
    let spec = result.as_ref().map(|r| chart(r, &meta));
    let spread_enabled = (plot.show_spread || plot.show_range)
        && result.as_ref().is_some_and(|r| {
            matches!(
                r.snapshot.plan.operation,
                Operation::History | Operation::Scaling { .. } | Operation::Variation { .. }
            ) && (matches!(r.snapshot.plan.operation, Operation::Variation { .. })
                || r.series
                    .iter()
                    .flat_map(|s| &s.points)
                    .any(|p| p.coverage.selected_inputs == 1))
        });
    let available_spread =
        spec.as_ref().is_some_and(|chart| {
            chart.series.iter().flat_map(|s| &s.points).any(|point| {
                point.y.is_some() && point.spread.as_ref().is_some_and(|s| s.count >= 2)
            })
        });
    let spread_label = spec
        .as_ref()
        .and_then(|chart| chart.spread_population.as_deref())
        .unwrap_or_default();
    let nonpositive = spec
        .as_ref()
        .map(|c| {
            c.series
                .iter()
                .flat_map(|s| &s.points)
                .filter(|p| p.y.is_some_and(|v| v <= 0.))
                .count()
        })
        .unwrap_or(0);
    let empty = spec.as_ref().is_some_and(|c| {
        c.series
            .iter()
            .flat_map(|s| &s.points)
            .all(|p| p.y.is_none())
    });
    rsx! {section{class:"chart-card",
        if let Some(r)=&result{
            header{class:"chart-heading",h2{"{title(&r.snapshot.plan.operation)}"}p{class:"muted","{r.inputs.len()} inputs · {r.revisions.len()} commits"}}
            for warning in &r.warnings{p{class:"notice",role:"status","{warning}"}}
            if let Some(reason)=plot.bounds_error(){p{class:"notice",role:"status","{reason} Bounds are ignored until corrected."}}
            if plot.y_scale==AxisScale::Log&&nonpositive>0{p{class:"notice",role:"status","{nonpositive} zero or negative values cannot be drawn on a logarithmic axis. They remain in the analysis."}}
            if empty{p{class:"notice",role:"status","No usable estimates for this selection. Inspect coverage and gaps below."}}
            if spread_enabled && available_spread {
                p { class: "chart-help", title: format!("Population: {spread_label}. These bars describe observed variability, not a confidence interval."),
                    if plot.show_range { "Thin bars: min–max. " }
                    if plot.show_spread { "Thick bars: mean ± 1 SD. " }
                    "Hover for values; click a point for the input and commit details."
                }
                if plot.y_scale == AxisScale::Log {
                    p {class:"muted", "Spread bars with zero or negative endpoints are omitted on the logarithmic axis."}
                }
            }
            if spread_enabled && !available_spread {
                    p { class:"chart-help",role:"status",
                        if !Reduction::estimates().contains(&r.snapshot.plan.inputs) && r.snapshot.plan.inputs != Reduction::GeometricMean {
                            "Custom averaging is active. Choose One line per input to use means and show repeat variability."
                        } else if matches!(r.snapshot.plan.operation, Operation::Variation { .. }) {
                            "At least two measurements are needed to show variability."
                        } else if r.series.iter().flat_map(|s| &s.points).any(|p| p.coverage.selected_inputs > 1) {
                            "Combined input estimates have no spread bars. Select one input or split series by Input identity to show repetitions of the same input. Workload spread is available under Variation → Across inputs."
                        } else if metric(&meta, &r.snapshot.plan).is_some_and(|m| matches!(m.source, MetricSource::Formula { stage: FormulaStage::Input | FormulaStage::Suite, .. })) {
                            "This metric combines averages, so repeat variability is unavailable. Runtime metrics have individual measurements."
                        } else if r.snapshot.plan.normalization.is_some() {
                            "Normalized estimates do not expose repetition spread. Disable normalization to inspect raw repetition variability."
                        } else if r.series.iter().flat_map(|s| &s.points).any(|p| p.coverage.runs > 1) || matches!(r.snapshot.plan.runs, Some(RunTreatment::Summaries { .. })) {
                            "Repetition bars use one run at a time. Separate runs to show their repetitions, or use Variation → Between runs."
                        } else {
                            "At least two successful repeats are needed to show min–max and standard deviation."
                        }
                    }
            }
            if matches!(r.snapshot.plan.operation,Operation::Coverage){p{class:"notice","One slot is an input at a revision in one environment/configuration. Differing selected-run states are marked mixed. The table retains individual run and attempt counts."}}
        }else{p{class:"chart-placeholder",role:"status",if loading{"Loading analysis..."}else{"Choose a metric, inputs and commits, then update the chart."}}}
        PlotlyChart{chart:spec,options:plot,on_point:on_select}
        if let Some(r)=result{ResultsTable{key:"{r.id}",result:r,meta:meta.clone(),on_select,on_page}}
    }}
}
#[component]
fn ResultsTable(
    result: AnalysisResponse,
    meta: Metadata,
    on_select: EventHandler<String>,
    on_page: EventHandler<Paging>,
) -> Element {
    let mut local_page = use_signal(|| 0usize);
    let mut only_problems = use_signal(|| false);
    let input_label = |id: InputId| {
        result
            .inputs
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.label.clone())
            .unwrap_or_else(|| format!("Input {id}"))
    };
    let plan = &result.snapshot.plan;
    match &plan.operation {
        Operation::Changes { .. } => {
            rsx! {section{class:"result-table",aria_label:"Per-input changes",
                h3{"Per-input changes"}
                for s in &result.change_summaries{
                    details{class:"change-summary",summary{"{result.groups.iter().find(|g|g.id==s.series).map(|g|label(&g.label)).unwrap_or_default()} · {short(&s.baseline)} → {short(&s.current)} · {s.matched}/{s.selected} matched · {s.improved} improved · {s.worsened} worsened"}
                        p{class:"muted","Suite: {number(s.before)} → {number(s.after)}; {number(s.percent)}%. Baseline only: {s.baseline_only}; current only: {s.current_only}; invalid/unmatched: {s.invalid}."}
                        p{class:"muted","Within threshold: {s.within_threshold}; unclassified: {s.unclassified}; invalid relative comparison: {s.unavailable}."}
                    }
                }
                div{class:"table-scroll",table{thead{tr{th{"Input"}th{"Revisions"}th{"Environment / run"}th{"Before"}th{"After"}th{"Absolute change"}th{"Relative change"}th{"Contribution"}th{"Classification"}}}tbody{
                    for row in &result.changes.items{tr{
                        td{button{class:"row-link",onclick:{let id=row.id.clone();move|_|on_select.call(id.clone())},"{input_label(row.input)}"}}
                        td{title:format!("{} → {}",row.baseline,row.current),"{short(&row.baseline)} → {short(&row.current)}" if !row.skipped_revisions.is_empty(){small{class:"muted"," {row.skipped_revisions.len()} revisions skipped"}}}
                        td{"{configuration_name(&meta,plan,&row.partition)}" if let Some(run)=row.run_group{small{class:"muted","Run {run}"}}}
                        td{"{number(row.before)}"}td{"{number(row.after)}"}td{"{number(row.absolute)}"}td{"{number(row.percent)}"}td{"{number(row.contribution)}"}
                        td{title:row.reason.clone().unwrap_or_default(),span{class:match row.class{ChangeClass::Worsened=>"badge bad",ChangeClass::Improved=>"badge good",_=>"badge"},"{change_class(row.class)}"}}
                    }}
                }}}
                Pager{offset:result.changes.offset,limit:plan.paging.limit,total:result.changes.total,on_change:on_page}
            }}
        }
        Operation::Variation { population } => {
            rsx! {section{class:"result-table",aria_label:"Variation summaries",h3{"Variation summaries"}p{class:"muted","Population: {population_label(*population)}. Counts refer to that population."}
                div{class:"table-scroll",table{thead{tr{th{"Revision"}th{"Input / group"}th{"Environment"}th{"Run"}th{"Count"}th{"Mean"}th{"Sample SD"}th{"Median"}th{"P90"}th{"P95"}th{"Minimum"}th{"Maximum"}}}tbody{
                    for row in &result.distributions.items{tr{
                        td{button{class:"row-link",onclick:{let id=row.id.clone();move|_|on_select.call(id.clone())},"{short(&row.revision)}"}}
                        td{if let Some(input)=row.input{"{input_label(input)}"}else if row.dimensions.is_empty(){"Input estimates"}else{"{encoded(&row.dimensions)}"}}
                        td{"{configuration_name(&meta,plan,&row.partition)}"}
                        td{{row.run.map(|r|r.to_string()).unwrap_or_else(||if *population==Population::BetweenRuns{"Per-run estimates".into()}else{"Selected work items".into()})}}td{"{row.count}"}td{"{number(row.mean)}"}td{"{number(row.standard_deviation)}"}td{"{number(row.median)}"}td{"{number(row.p90)}"}td{"{number(row.p95)}"}td{"{number(row.min)}"}td{"{number(row.max)}"}
                    }}
                }}}
                Pager{offset:result.distributions.offset,limit:plan.paging.limit,total:result.distributions.total,on_change:on_page}
            }}
        }
        Operation::Coverage => {
            let rows = result
                .coverage
                .iter()
                .filter(|c| {
                    !only_problems()
                        || coverage_status(c) != "Measured"
                        || c.outcomes.missing_metric > 0
                })
                .collect::<Vec<_>>();
            let total = rows.len();
            let offset = local_page().min(total.saturating_sub(1) / 50 * 50);
            rsx! {section{class:"result-table",aria_label:"Execution coverage",h3{"Execution and metric coverage"}
                label{class:"check-row",input{r#type:"checkbox",checked:only_problems(),onchange:move|e|{only_problems.set(e.checked());local_page.set(0);}}"Show only missing or unsuccessful inputs"}
                div{class:"table-scroll",table{thead{tr{th{"Input"}th{"Revision"}th{"Environment"}th{"Run"}th{"Work item"}th{"Success"}th{"Failure"}th{"Unsupported"}th{"Unfinished attempts"}th{"Unstarted repetitions"}th{"Missing metric"}}}tbody{
                    for row in rows.into_iter().skip(offset).take(50){tr{td{button{class:"row-link",onclick:{let id=row.id.clone();move|_|on_select.call(id.clone())},"{input_label(row.input)}"}}td{"{short(&row.revision)}"}td{"{configuration_name(&meta,plan,&row.partition)}"}td{{row.run.map(|r|r.to_string()).unwrap_or_else(||"Unmeasured".into())}}
                        td{title:row.reason.clone().unwrap_or_default(),span{class:if coverage_status(row)=="Measured"{"badge good"}else{"badge"},"{coverage_status(row)}"}if let Some(reason)=&row.reason{small{class:"muted","{reason}"}}if !row.newer_unfinished.is_empty(){small{class:"muted","Newer unfinished work exists"}}}
                        td{"{row.outcomes.success}"}td{"{row.outcomes.failure}"}td{"{row.outcomes.unsupported}"}td{"{row.outcomes.unfinished}"}td{"{row.outcomes.missing}"}td{"{row.outcomes.missing_metric}"}
                    }}
                }}}
                Pager{offset,limit:50,total,on_change:move|p:Paging|local_page.set(p.offset)}
            }}
        }
        _ => {
            let rows = result
                .series
                .iter()
                .flat_map(|s| s.points.iter().map(move |p| (s, p)))
                .collect::<Vec<_>>();
            let total = rows.len();
            let offset = local_page().min(total.saturating_sub(1) / 50 * 50);
            rsx! {section{class:"result-table",aria_label:"Chart values and gaps",details{summary{"Chart values and gaps ({total})"}
                div{class:"table-scroll",table{thead{tr{th{"Series"}th{"Revision / parameter"}th{"Estimate"}th{"Inputs"}th{"Valid repetitions"}th{"Runs"}th{"Exclusions"}}}tbody{
                    for (series,point) in rows.into_iter().skip(offset).take(50){tr{td{"{series.label}"}td{button{class:"row-link",onclick:{let id=point.id.clone();move|_|on_select.call(id.clone())},"{value_text(&point.x)}"}}td{"{number(point.value)}"}td{"{point.coverage.included_inputs}/{point.coverage.selected_inputs}"}td{"{point.coverage.valid_repetitions}"}td{"{point.coverage.runs}"}td{if point.coverage.exclusions.is_empty(){"None"}else{details{summary{"{point.coverage.exclusions.len()} excluded"}ul{for ex in &point.coverage.exclusions{li{"{input_label(ex.input)}: {ex.reason}"}}}}}}}}
                }}}
                Pager{offset,limit:50,total,on_change:move|p:Paging|local_page.set(p.offset)}
            }}}
        }
    }
}
