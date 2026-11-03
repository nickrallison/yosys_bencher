use super::*;

pub(super) fn sample<O: BenchmarkOut>(
    attempt: &StoredAttempt<O>,
    work: &WorkItemSummary,
    metric: &MetricSpec,
    metrics: &[MetricSpec],
    plan: &AnalysisPlan,
    cache: &mut BTreeMap<AttemptId, Sample>,
) -> Sample {
    cache
        .entry(attempt.id)
        .or_insert_with(|| extract_sample(attempt, work, metric, metrics, plan))
        .clone()
}

fn extract_sample<O: BenchmarkOut>(
    attempt: &StoredAttempt<O>,
    work: &WorkItemSummary,
    metric: &MetricSpec,
    metrics: &[MetricSpec],
    plan: &AnalysisPlan,
) -> Sample {
    let mut observation = Observation {
        attempt: attempt.id,
        run: work.run,
        work_item: work.id,
        repetition: attempt.sample_index,
        started_at: attempt.started_at,
        value: None,
        operands: None,
        reason: None,
        identities: vec![],
    };
    let result = (|| -> std::result::Result<(f64, Option<(f64, f64)>), String> {
        let record = attempt
            .record
            .as_ref()
            .ok_or_else(|| format!("Attempt {:?}", attempt.state))?;
        let outcome = match &record.outcome {
            AttemptOutcome::Success(_) => "success",
            AttemptOutcome::Failure(_) => "failure",
            AttemptOutcome::Unsupported(_) => "unsupported",
        };
        for f in &plan.filters {
            if matches!(f.stage, FilterStage::Attempt) && !f.filter.matches(Some(&json!(outcome))) {
                return Err("Excluded by attempt outcome filter".into());
            }
        }
        let output = match &record.outcome {
            AttemptOutcome::Success(o) => o,
            AttemptOutcome::Failure(reason) => return Err(format!("Failed: {reason}")),
            AttemptOutcome::Unsupported(reason) => return Err(format!("Unsupported: {reason}")),
        };
        let mut extract = |path: &str| -> std::result::Result<f64, String> {
            observation
                .identities
                .push(match output.observation_identity(path) {
                    Some(source) => format!("run:{}:{source}", work.run),
                    None => format!("attempt:{}:{path}", attempt.id),
                });
            let v = scalar(output, path)
                .ok_or_else(|| format!("Missing or nonfinite metric: {path}"))?;
            let domain = metrics
                .iter()
                .find(|m| m.path == path)
                .map(|m| m.domain)
                .unwrap_or(Domain::Finite);
            if domain.accepts(v) {
                Ok(v)
            } else {
                Err(format!("Metric outside valid domain: {path}"))
            }
        };
        match &metric.source {
            MetricSource::Path => Ok((extract(&metric.path)?, None)),
            MetricSource::Formula {
                numerator,
                denominator,
                formula,
                stage,
            } => {
                let a = extract(numerator)?;
                let b = extract(denominator)?;
                if *stage == FormulaStage::Repetition {
                    Ok((
                        formula
                            .apply(a, b)
                            .ok_or("Invalid formula denominator or result")?,
                        None,
                    ))
                } else {
                    Ok((a, Some((a, b))))
                }
            }
        }
    })();
    let operands = match result {
        Ok((value, operands)) if operands.is_some() || metric.domain.accepts(value) => {
            observation.value = if operands.is_some() {
                None
            } else {
                Some(value)
            };
            observation.operands = operands;
            operands
        }
        Ok(_) => {
            observation.reason = Some("Metric outside valid domain".into());
            None
        }
        Err(reason) => {
            observation.reason = Some(reason);
            None
        }
    };
    Sample {
        observation,
        operands,
    }
}

pub(super) fn has_value(e: &Estimate, m: &MetricSpec) -> bool {
    if matches!(
        m.source,
        MetricSource::Formula {
            stage: FormulaStage::Suite,
            ..
        }
    ) {
        e.operands.is_some()
    } else {
        e.value.is_some()
    }
}
fn summarize(
    samples: Vec<Sample>,
    mut estimate: Estimate,
    metric: &MetricSpec,
    reduction: Reduction,
) -> Estimated {
    let mut seen = BTreeSet::new();
    let valid: Vec<_> = samples
        .iter()
        .filter(|s| s.observation.reason.is_none())
        .filter(|s| seen.insert(s.observation.identities.clone()))
        .collect();
    let values: Vec<_> = valid.iter().filter_map(|s| s.observation.value).collect();
    estimate.valid_repetitions = valid.len();
    estimate.value = match &metric.source {
        MetricSource::Formula { formula, stage, .. } if *stage != FormulaStage::Repetition => {
            let a: Vec<_> = valid
                .iter()
                .filter_map(|s| s.operands.map(|p| p.0))
                .collect();
            let b: Vec<_> = valid
                .iter()
                .filter_map(|s| s.operands.map(|p| p.1))
                .collect();
            estimate.operands = reduction.apply(&a).zip(reduction.apply(&b));
            estimate.operands.and_then(|(a, b)| formula.apply(a, b))
        }
        _ => reduction.apply(&values),
    };
    estimate.standard_deviation = if matches!(
        metric.source,
        MetricSource::Path
            | MetricSource::Formula {
                stage: FormulaStage::Repetition,
                ..
            }
    ) {
        Reduction::StdDev.apply(&values)
    } else {
        None
    };
    estimate
        .reasons
        .extend(samples.iter().filter_map(|s| s.observation.reason.clone()));
    if !has_value(&estimate, metric) {
        estimate
            .reasons
            .push("Insufficient samples, invalid formula, or invalid reduction domain".into());
    }
    if !matches!(
        metric.source,
        MetricSource::Formula {
            stage: FormulaStage::Suite,
            ..
        }
    ) && estimate.value.is_some_and(|v| !metric.domain.accepts(v))
    {
        estimate.value = None;
        estimate
            .reasons
            .push("Estimate outside metric domain".into());
    }
    Estimated { estimate, samples }
}
fn input_filters(estimated: &mut Estimated, plan: &AnalysisPlan) {
    if plan
        .filters
        .iter()
        .filter(|f| matches!(f.stage, FilterStage::Input))
        .any(|f| {
            !f.filter
                .matches(estimated.estimate.value.as_ref().map(|v| json!(v)).as_ref())
        })
    {
        estimated.estimate.value = None;
        estimated
            .estimate
            .reasons
            .push("Excluded by per-input estimate filter".into());
    }
}
pub(super) fn estimates<I: BenchmarkIn, O: BenchmarkOut>(
    frame: &ResultFrame<I, O>,
    metric: &MetricSpec,
    metrics: &[MetricSpec],
    plan: &AnalysisPlan,
    cache: &mut BTreeMap<AttemptId, Sample>,
) -> (Vec<Estimated>, BTreeMap<Key, Estimated>) {
    let mut per_run = Vec::new();
    for work in &frame.work_items {
        let partition = frame
            .runs
            .iter()
            .find(|r| r.id == work.run)
            .unwrap()
            .partition
            .clone();
        let samples = frame
            .attempts
            .iter()
            .filter(|a| a.work_item == work.id)
            .map(|a| sample(a, work, metric, metrics, plan, cache))
            .collect();
        let mut reasons = Vec::new();
        if let Some(reason) = &work.reason {
            reasons.push(reason.clone());
        }
        if !work.state.terminal() {
            reasons.push(format!("Work item {:?}; partial measurements", work.state));
        }
        let newer_unfinished = frame
            .newer_unfinished
            .iter()
            .filter(|w| {
                w.input == work.input
                    && w.revision == work.revision
                    && frame
                        .runs
                        .iter()
                        .find(|r| r.id == w.run)
                        .is_some_and(|r| r.partition == partition)
            })
            .map(|w| w.id)
            .collect();
        let e = Estimate {
            id: id(&(work.id, &metric.path)),
            revision: work.revision.clone(),
            input: work.input,
            partition,
            run_group: Some(work.run),
            run_ids: vec![work.run],
            work_items: vec![work.id],
            value: None,
            standard_deviation: None,
            valid_repetitions: 0,
            planned_repetitions: work.planned_repetitions as usize,
            reasons,
            dependencies: vec![],
            newer_unfinished,
            operands: None,
        };
        let mut d = summarize(samples, e, metric, plan.repetitions);
        if !work.state.terminal() && matches!(plan.selection.runs, RunSelection::LatestCompleted) {
            d.estimate.value = None;
            d.estimate.operands = None;
        }
        per_run.push(d);
    }
    let separate = matches!(plan.runs, Some(RunTreatment::Separate))
        || plan.series.iter().any(|d| matches!(d, Dimension::Run));
    let mut groups = BTreeMap::<Key, Vec<Estimated>>::new();
    for data in &per_run {
        let e = &data.estimate;
        groups
            .entry((
                e.partition.clone(),
                e.revision.clone(),
                e.input,
                if separate { e.run_group } else { None },
            ))
            .or_default()
            .push(data.clone());
    }
    let combined = groups
        .into_iter()
        .map(|(key, mut group)| {
            let mut merged = group[0].clone();
            merged.estimate.run_group = key.3;
            if group.len() > 1 {
                let samples = group.iter().flat_map(|d| d.samples.clone()).collect();
                merged.estimate.run_ids = group
                    .iter()
                    .flat_map(|d| d.estimate.run_ids.clone())
                    .collect();
                merged.estimate.work_items = group
                    .iter()
                    .flat_map(|d| d.estimate.work_items.clone())
                    .collect();
                merged.estimate.planned_repetitions =
                    group.iter().map(|d| d.estimate.planned_repetitions).sum();
                merged.estimate.newer_unfinished = group
                    .iter()
                    .flat_map(|d| d.estimate.newer_unfinished.clone())
                    .collect();
                merged.estimate.reasons = group
                    .iter()
                    .flat_map(|d| d.estimate.reasons.clone())
                    .collect();
                merged = summarize(samples, merged.estimate, metric, plan.repetitions);
                if let Some(RunTreatment::Summaries { reduction }) = plan.runs {
                    let values: Vec<_> = group.iter().filter_map(|d| d.estimate.value).collect();
                    merged.estimate.value = reduction.apply(&values);
                    merged.estimate.standard_deviation = None;
                    if matches!(
                        metric.source,
                        MetricSource::Formula {
                            stage: FormulaStage::Suite,
                            ..
                        }
                    ) {
                        let pairs: Vec<_> =
                            group.iter().filter_map(|d| d.estimate.operands).collect();
                        merged.estimate.operands = reduction
                            .apply(&pairs.iter().map(|p| p.0).collect::<Vec<_>>())
                            .zip(reduction.apply(&pairs.iter().map(|p| p.1).collect::<Vec<_>>()));
                    }
                }
            } else {
                merged = group.remove(0);
                merged.estimate.run_group = key.3;
            }
            merged.estimate.id = id(&(&key, &metric.path));
            input_filters(&mut merged, plan);
            (key, merged)
        })
        .collect();
    for data in &mut per_run {
        input_filters(data, plan);
    }
    (per_run, combined)
}
