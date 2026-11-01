use super::*;

pub(super) fn membership(
    group: &Group,
    revisions: &[String],
    data: &BTreeMap<Key, Estimated>,
    metric: &MetricSpec,
    policy: CoveragePolicy,
) -> BTreeSet<InputId> {
    if policy != CoveragePolicy::Common {
        return group.inputs.clone();
    }
    group
        .inputs
        .iter()
        .copied()
        .filter(|&input| {
            revisions.iter().all(|r| {
                data.get(&(group.partition.clone(), r.clone(), input, group.run))
                    .is_some_and(|e| has_value(&e.estimate, metric))
            })
        })
        .collect()
}
pub(super) fn point(
    group: &Group,
    revision: &str,
    eligible: &BTreeSet<InputId>,
    data: &BTreeMap<Key, Estimated>,
    metric: &MetricSpec,
    plan: &AnalysisPlan,
) -> Point {
    let mut values = Vec::new();
    let mut exclusions = Vec::new();
    let mut valid_repetitions = 0;
    let mut planned_repetitions = 0;
    let mut runs = BTreeSet::new();
    let mut contributors = Vec::new();
    for &input in &group.inputs {
        let datum = data.get(&(group.partition.clone(), revision.into(), input, group.run));
        if let Some(d) = datum {
            contributors.push(d.estimate.id.clone());
            contributors.extend(d.estimate.dependencies.clone());
            planned_repetitions += d.estimate.planned_repetitions;
        }
        if !eligible.contains(&input) {
            exclusions.push(Exclusion {
                input,
                reason: "Not in fixed common set".into(),
            });
            continue;
        }
        match datum {
            Some(d) if has_value(&d.estimate, metric) => {
                values.push(&d.estimate);
                valid_repetitions += d.estimate.valid_repetitions;
                runs.extend(d.estimate.run_ids.iter().copied());
            }
            Some(d) => exclusions.push(Exclusion {
                input,
                reason: d.estimate.reasons.join("; "),
            }),
            None => exclusions.push(Exclusion {
                input,
                reason: "Unmeasured in selected runs".into(),
            }),
        }
    }
    let complete = values.len() == eligible.len();
    let value = if plan.coverage == CoveragePolicy::Strict && !complete {
        None
    } else {
        aggregate(&values, metric, plan.inputs)
    };
    let mut details = BTreeMap::new();
    details.insert("revision".into(), revision.into());
    if value.is_none()
        && !values.is_empty()
        && (plan.coverage != CoveragePolicy::Strict || complete)
    {
        details.insert("gap".into(), "Invalid reduction domain or formula".into());
    }
    Point {
        id: id(&(&group.id, revision)),
        x: json!(revision),
        value,
        coverage: Coverage {
            selected_inputs: group.inputs.len(),
            eligible_inputs: eligible.len(),
            included_inputs: values.len(),
            valid_repetitions,
            planned_repetitions,
            runs: runs.len(),
            exclusions,
        },
        contributors,
        details,
    }
}
pub(super) fn history(
    groups: &[Group],
    revisions: &[String],
    data: &BTreeMap<Key, Estimated>,
    metric: &MetricSpec,
    plan: &AnalysisPlan,
) -> Vec<Series> {
    groups
        .iter()
        .map(|g| {
            let eligible = membership(g, revisions, data, metric, plan.coverage);
            Series {
                partition: g.partition.clone(),
                id: g.id.clone(),
                label: g.label.clone(),
                dimensions: g.dimensions.clone(),
                points: revisions
                    .iter()
                    .map(|r| point(g, r, &eligible, data, metric, plan))
                    .collect(),
            }
        })
        .collect()
}
