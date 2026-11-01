use super::*;

pub(super) fn changes(
    groups: &[Group],
    revisions: &[String],
    data: &BTreeMap<Key, Estimated>,
    metric: &MetricSpec,
    plan: &AnalysisPlan,
) -> Result<(Vec<ChangeRow>, Vec<ChangeSummary>)> {
    let Operation::Changes {
        comparison,
        threshold,
        ranking,
    } = &plan.operation
    else {
        return Err(invalid("operation", "expected Changes"));
    };
    let (threshold, ranking) = (*threshold, *ranking);
    if ranking == Ranking::Contribution
        && (plan.inputs != Reduction::Mean
            || matches!(
                metric.source,
                MetricSource::Formula {
                    stage: FormulaStage::Suite,
                    ..
                }
            ))
    {
        return Err(invalid(
            "ranking",
            "contributions require an equal-weight arithmetic mean of per-input values",
        ));
    }
    let mut rows = Vec::new();
    let mut summaries = Vec::new();
    for group in groups {
        let eligible = membership(group, revisions, data, metric, plan.coverage);
        let pairs: Vec<(String, String, Vec<String>)> = match comparison {
            Comparison::Pair { baseline, current } => {
                vec![(baseline.clone(), current.clone(), vec![])]
            }
            Comparison::AdjacentSelected => revisions
                .windows(2)
                .map(|r| (r[0].clone(), r[1].clone(), vec![]))
                .collect(),
            Comparison::PreviousMeasured => {
                let mut prev: Option<usize> = None;
                let mut pairs = Vec::new();
                for (i, r) in revisions.iter().enumerate() {
                    let measured = group.inputs.iter().any(|&input| {
                        data.get(&(group.partition.clone(), r.clone(), input, group.run))
                            .is_some_and(|d| d.estimate.value.is_some())
                    });
                    if measured {
                        if let Some(p) = prev {
                            pairs.push((
                                revisions[p].clone(),
                                r.clone(),
                                revisions[p + 1..i].to_vec(),
                            ));
                        }
                        prev = Some(i);
                    }
                }
                pairs
            }
        };
        for (baseline, current, skipped) in pairs {
            let mut summary = ChangeSummary {
                series: group.id.clone(),
                baseline: baseline.clone(),
                current: current.clone(),
                partition: group.partition.clone(),
                run_group: group.run,
                selected: group.inputs.len(),
                matched: 0,
                baseline_only: 0,
                current_only: 0,
                invalid: 0,
                improved: 0,
                within_threshold: 0,
                worsened: 0,
                unclassified: 0,
                unavailable: 0,
                before: None,
                after: None,
                absolute: None,
                percent: None,
            };
            let mut before_values = Vec::new();
            let mut after_values = Vec::new();
            let start = rows.len();
            for &input in &group.inputs {
                let before =
                    data.get(&(group.partition.clone(), baseline.clone(), input, group.run));
                let after = data.get(&(group.partition.clone(), current.clone(), input, group.run));
                let a = before.and_then(|d| d.estimate.value);
                let b = after.and_then(|d| d.estimate.value);
                let mut contributors = Vec::new();
                for d in [before, after].into_iter().flatten() {
                    contributors.push(d.estimate.id.clone());
                }
                let matched = a.zip(b).filter(|_| eligible.contains(&input));
                let (absolute, percent, classification, reason) = if let Some((a, b)) = matched {
                    summary.matched += 1;
                    before_values.push(&before.unwrap().estimate);
                    after_values.push(&after.unwrap().estimate);
                    let c = class(a, b, threshold, metric.direction);
                    match c {
                        ChangeClass::Improved => summary.improved += 1,
                        ChangeClass::WithinThreshold => summary.within_threshold += 1,
                        ChangeClass::Worsened => summary.worsened += 1,
                        ChangeClass::Unclassified => summary.unclassified += 1,
                        ChangeClass::Unavailable => summary.unavailable += 1,
                    }
                    (
                        ChangeKind::Absolute.apply(b, a),
                        ChangeKind::Percent.apply(b, a),
                        c,
                        (c == ChangeClass::Unavailable)
                            .then(|| "Invalid relative-change denominator".into()),
                    )
                } else {
                    match (a, b) {
                        (Some(_), None) => summary.baseline_only += 1,
                        (None, Some(_)) => summary.current_only += 1,
                        _ => summary.invalid += 1,
                    }
                    let reason = if !eligible.contains(&input) {
                        "Not in fixed common set".into()
                    } else {
                        [before, after]
                            .into_iter()
                            .flatten()
                            .flat_map(|d| d.estimate.reasons.clone())
                            .collect::<Vec<_>>()
                            .join("; ")
                    };
                    (
                        None,
                        None,
                        ChangeClass::Unavailable,
                        Some(if reason.is_empty() {
                            "Missing comparison endpoint".into()
                        } else {
                            reason
                        }),
                    )
                };
                rows.push(ChangeRow {
                    id: id(&(&group.id, &baseline, &current, input)),
                    series: group.id.clone(),
                    input,
                    partition: group.partition.clone(),
                    run_group: group.run,
                    baseline: baseline.clone(),
                    current: current.clone(),
                    before: a,
                    after: b,
                    absolute,
                    percent,
                    contribution: None,
                    class: classification,
                    reason,
                    skipped_revisions: skipped.clone(),
                    contributors,
                });
            }
            let complete = summary.matched == eligible.len();
            if plan.coverage != CoveragePolicy::Strict || complete {
                summary.before = aggregate(&before_values, metric, plan.inputs);
                summary.after = aggregate(&after_values, metric, plan.inputs);
                if let Some((a, b)) = summary.before.zip(summary.after) {
                    summary.absolute = ChangeKind::Absolute.apply(b, a);
                    summary.percent = ChangeKind::Percent.apply(b, a);
                }
                if plan.inputs == Reduction::Mean
                    && !matches!(
                        metric.source,
                        MetricSource::Formula {
                            stage: FormulaStage::Suite,
                            ..
                        }
                    )
                    && summary.matched > 0
                {
                    for row in &mut rows[start..] {
                        row.contribution = row.absolute.map(|v| v / summary.matched as f64);
                    }
                }
            }
            summaries.push(summary);
        }
    }
    let score = |row: &ChangeRow| {
        match ranking {
            Ranking::Absolute => row.absolute,
            Ranking::Percent => row.percent,
            Ranking::Contribution => row.contribution,
        }
        .map(|v| {
            if metric.direction == Direction::Higher {
                -v
            } else {
                v
            }
        })
    };
    rows.sort_by(|a, b| {
        match (score(a), score(b)) {
            (Some(a), Some(b)) => b.total_cmp(&a),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
        .then_with(|| a.id.cmp(&b.id))
    });
    Ok((rows, summaries))
}
