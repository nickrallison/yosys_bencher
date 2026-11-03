mod changes;
mod coverage;
mod estimates;
mod groups;
mod history;
mod scaling;
mod validation;
mod variation;
use changes::changes;
use coverage::coverage;
use estimates::{estimates, has_value, sample};
use groups::{Group, groups};
use history::{history, membership, point};
use scaling::scaling;
pub(crate) use validation::validate;
use variation::variation;

use super::{plan::*, result::*};
use crate::{
    experiment::{AttemptOutcome, BenchmarkIn, BenchmarkOut, FieldKind, Filter, InputField, Page},
    metrics::*,
    store::{self, Result, *},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Key = (String, String, InputId, Option<RunId>);
#[derive(Clone)]
struct Sample {
    observation: Observation,
    operands: Option<(f64, f64)>,
}
#[derive(Clone)]
struct Estimated {
    estimate: Estimate,
    samples: Vec<Sample>,
}
pub(crate) struct Evaluation {
    pub response: AnalysisResponse,
    pub estimates: Vec<Estimate>,
    pub observations: BTreeMap<String, Vec<Observation>>,
}

fn invalid(field: &str, message: impl std::fmt::Display) -> StoreError {
    StoreError::Validation {
        field: field.into(),
        message: message.to_string(),
    }
}
fn id(value: &impl serde::Serialize) -> String {
    store::fingerprint(value).expect("analysis identity contains only finite values and strings")
}
fn aggregate(values: &[&Estimate], metric: &MetricSpec, reduction: Reduction) -> Option<f64> {
    if let MetricSource::Formula {
        formula,
        stage: FormulaStage::Suite,
        ..
    } = metric.source
    {
        let pairs: Vec<_> = values.iter().filter_map(|e| e.operands).collect();
        if pairs.len() != values.len() {
            return None;
        }
        formula.apply(
            reduction.apply(&pairs.iter().map(|p| p.0).collect::<Vec<_>>())?,
            reduction.apply(&pairs.iter().map(|p| p.1).collect::<Vec<_>>())?,
        )
    } else {
        reduction.apply(&values.iter().filter_map(|e| e.value).collect::<Vec<_>>())
    }
}
fn class(before: f64, after: f64, threshold: Threshold, direction: Direction) -> ChangeClass {
    let Some(change) = threshold.kind.apply(after, before) else {
        return ChangeClass::Unavailable;
    };
    if direction == Direction::Unspecified {
        return ChangeClass::Unclassified;
    }
    if change.abs() <= threshold.value {
        return ChangeClass::WithinThreshold;
    }
    if (change > 0.0) == (direction == Direction::Lower) {
        ChangeClass::Worsened
    } else {
        ChangeClass::Improved
    }
}

pub fn analyze_frame<I: BenchmarkIn, O: BenchmarkOut>(
    frame: &ResultFrame<I, O>,
    plan: &AnalysisPlan,
) -> Result<AnalysisResponse> {
    let revisions = frame
        .revisions
        .iter()
        .map(|r| r.id().id().clone())
        .collect::<Vec<_>>();
    let metrics = crate::metrics::catalog(frame.attempts.iter().filter_map(|attempt| {
        match &attempt.record.as_ref()?.outcome {
            AttemptOutcome::Success(output) => Some(output),
            _ => None,
        }
    }));
    evaluate(
        frame,
        plan,
        &revisions,
        plan.normalization.as_ref().map(|_| frame),
        plan.cohort.as_ref().map(|_| frame),
        &metrics,
    )
    .map(|e| e.response)
}

pub(crate) fn evaluate<I: BenchmarkIn, O: BenchmarkOut>(
    frame: &ResultFrame<I, O>,
    plan: &AnalysisPlan,
    revisions: &[String],
    baseline: Option<&ResultFrame<I, O>>,
    cohort_frame: Option<&ResultFrame<I, O>>,
    metrics: &[MetricSpec],
) -> Result<Evaluation> {
    let metric = validate::<I>(plan, metrics)?;
    for supplemental in [baseline, cohort_frame].into_iter().flatten() {
        if supplemental.generation != frame.generation {
            return Err(StoreError::Expired);
        }
    }
    let mut samples = BTreeMap::new();
    let snapshot = Snapshot {
        generation: frame.generation,
        plan: plan.clone(),
    };
    let mut response = AnalysisResponse {
        groups: vec![],
        id: id(&snapshot),
        snapshot,
        definition: frame.definition.clone(),
        selected_runs: frame
            .work_items
            .iter()
            .map(|w| w.run)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        selected_work_items: frame.work_items.iter().map(|w| w.id).collect(),
        unit: None,
        reference: None,
        inputs: frame
            .input_order
            .iter()
            .map(|id| frame.descriptions[id].clone())
            .collect(),
        revisions: revisions.to_vec(),
        partitions: frame
            .runs
            .iter()
            .map(|r| r.partition.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        series: vec![],
        changes: Page::from_items([], 0, 0),
        change_summaries: vec![],
        distributions: Page::from_items([], 0, 0),
        coverage: coverage(frame, plan, metric.as_ref(), metrics, &mut samples),
        warnings: frame
            .inputs
            .values()
            .flat_map(BenchmarkIn::warnings)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    };
    if !frame.newer_unfinished.is_empty() {
        response.warnings.push(
            "Newer unfinished work exists; selected completed work items are retained".into(),
        );
    }
    let Some(metric) = metric else {
        return Ok(Evaluation {
            response,
            estimates: vec![],
            observations: BTreeMap::new(),
        });
    };
    response.unit = Some(if plan.inputs == Reduction::Count {
        "inputs".into()
    } else {
        metric.unit.clone()
    });
    response.reference = if matches!(
        plan.inputs,
        Reduction::Sum | Reduction::Count | Reduction::StdDev
    ) {
        None
    } else {
        metric.reference
    };
    let (per_run, mut data) = estimates(frame, &metric, metrics, plan, &mut samples);
    let mut catalog = BTreeMap::<String, Estimated>::new();
    for d in per_run.iter().chain(data.values()) {
        catalog.insert(d.estimate.id.clone(), d.clone());
    }
    let mut groups = groups(frame, plan)?;
    if let Some(cohort) = &plan.cohort {
        let reference = cohort_frame.ok_or_else(|| invalid("cohort", "missing endpoint data"))?;
        let (_, cohort_data) = estimates(reference, &metric, metrics, plan, &mut samples);
        let mut active = BTreeSet::new();
        for (key, a) in &cohort_data {
            if key.1 != cohort.baseline {
                continue;
            }
            if let Some((a, b)) = a.estimate.value.zip(
                cohort_data
                    .get(&(key.0.clone(), cohort.current.clone(), key.2, key.3))
                    .and_then(|d| d.estimate.value),
            ) && class(a, b, cohort.threshold, metric.direction) == cohort.class
            {
                active.insert((key.0.clone(), key.2, key.3));
            }
        }
        for g in &mut groups {
            g.inputs
                .retain(|&input| active.contains(&(g.partition.clone(), input, g.run)));
        }
        groups.retain(|g| !g.inputs.is_empty());
        response
            .inputs
            .retain(|d| groups.iter().any(|g| g.inputs.contains(&d.id)));
        response.coverage.retain(|c| {
            groups
                .iter()
                .any(|g| g.partition == c.partition && g.inputs.contains(&c.input))
        });
        response.warnings.push(format!(
            "Fixed cohort resolved to {} input identities",
            response.inputs.len()
        ));
    }
    if let Some(normalization) = &plan.normalization {
        let baseline = baseline.ok_or_else(|| invalid("normalization", "missing baseline data"))?;
        let (_, mut baseline_data) = estimates(baseline, &metric, metrics, plan, &mut samples);
        for d in baseline_data.values_mut() {
            d.estimate.id = id(&("baseline", &d.estimate.id));
            catalog.insert(d.estimate.id.clone(), d.clone());
        }
        for (key, d) in &mut data {
            let base =
                baseline_data.get(&(key.0.clone(), normalization.revision.clone(), key.2, key.3));
            if let Some(base) = base {
                d.estimate.dependencies.push(base.estimate.id.clone());
            }
            d.estimate.value = d
                .estimate
                .value
                .zip(base.and_then(|d| d.estimate.value))
                .and_then(|(v, b)| normalization.kind.apply(v, b));
            d.estimate.standard_deviation = None;
            if d.estimate.value.is_none() {
                d.estimate
                    .reasons
                    .push("Missing or invalid matched revision baseline".into());
            }
        }
        response.unit = Some(match normalization.kind {
            ChangeKind::Ratio => "ratio".into(),
            ChangeKind::Percent => "%".into(),
            ChangeKind::Absolute => metric.unit.clone(),
        });
        response.reference = Some(if normalization.kind == ChangeKind::Ratio {
            1.0
        } else {
            0.0
        });
    }
    response.groups = groups
        .iter()
        .map(|g| SeriesDescription {
            partition: g.partition.clone(),
            id: g.id.clone(),
            label: g.label.clone(),
            dimensions: g.dimensions.clone(),
        })
        .collect();
    match &plan.operation {
        Operation::History => response.series = history(&groups, revisions, &data, &metric, plan),
        Operation::Changes { ranking, .. } => {
            let (rows, summaries) = changes(&groups, revisions, &data, &metric, plan)?;
            let filtered = rows.into_iter().filter(|r| {
                let value = match ranking {
                    Ranking::Absolute => r.absolute,
                    Ranking::Percent => r.percent,
                    Ranking::Contribution => r.contribution,
                };
                plan.display_filters
                    .iter()
                    .all(|f| f.matches(value.as_ref().map(|v| json!(v)).as_ref()))
            });
            response.changes = Page::from_items(filtered, plan.paging.offset, plan.paging.limit);
            response.change_summaries = summaries;
        }
        Operation::Scaling { .. } => {
            response.series = scaling(frame, &groups, revisions, &data, &metric, plan)?;
        }
        Operation::Variation { population } => {
            response.distributions = Page::from_items(
                variation(
                    &groups,
                    revisions,
                    &per_run,
                    &data,
                    *population,
                    &metric,
                    plan,
                )
                .into_iter()
                .filter(|d| {
                    plan.display_filters
                        .iter()
                        .all(|f| f.matches(d.mean.as_ref().map(|v| json!(v)).as_ref()))
                }),
                plan.paging.offset,
                plan.paging.limit,
            )
        }
        Operation::Coverage => (),
    }
    if !matches!(plan.operation, Operation::Changes { .. }) {
        for s in &mut response.series {
            s.points.retain(|p| {
                plan.display_filters
                    .iter()
                    .all(|f| f.matches(p.value.as_ref().map(|v| json!(v)).as_ref()))
            });
        }
    }
    if plan.inputs == Reduction::Count
        && !matches!(
            metric.source,
            MetricSource::Formula {
                stage: FormulaStage::Suite,
                ..
            }
        )
    {
        response.unit = Some("inputs".into());
        response.reference = None;
    }
    if plan.coverage == CoveragePolicy::Common
        && response
            .series
            .iter()
            .flat_map(|s| &s.points)
            .all(|p| p.coverage.eligible_inputs == 0)
        && !response.series.is_empty()
    {
        response
            .warnings
            .push("The fixed common input set is empty".into());
    }
    for d in data.values() {
        catalog.insert(d.estimate.id.clone(), d.clone());
    }
    Ok(Evaluation {
        response,
        estimates: catalog.values().map(|d| d.estimate.clone()).collect(),
        observations: catalog
            .into_iter()
            .map(|(k, d)| (k, d.samples.into_iter().map(|s| s.observation).collect()))
            .collect(),
    })
}
