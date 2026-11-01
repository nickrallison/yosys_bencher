use super::*;

pub(super) fn scaling<I: BenchmarkIn, O>(
    frame: &ResultFrame<I, O>,
    groups: &[Group],
    revisions: &[String],
    data: &BTreeMap<Key, Estimated>,
    metric: &MetricSpec,
    plan: &AnalysisPlan,
) -> Result<Vec<Series>> {
    let Operation::Scaling {
        x,
        reduce_remaining,
    } = &plan.operation
    else {
        return Err(invalid("operation", "expected Scaling"));
    };
    let reduce_remaining = *reduce_remaining;
    let retained: BTreeSet<_> = plan
        .series
        .iter()
        .filter_map(|d| {
            if let Dimension::Parameter(p) = d {
                Some(p.as_str())
            } else {
                None
            }
        })
        .collect();
    let mut series = Vec::new();
    for group in groups {
        if !reduce_remaining {
            for field in I::fields()
                .iter()
                .filter(|f| f.path != *x && !retained.contains(f.path.as_str()))
            {
                let values: BTreeSet<_> = group
                    .inputs
                    .iter()
                    .map(|id| {
                        store::encode(&frame.descriptions[id].parameters.get(&field.path)).unwrap()
                    })
                    .collect();
                if values.len() > 1 {
                    return Err(invalid(
                        "operation.reduce_remaining",
                        format!(
                            "fix, group, or explicitly reduce varying parameter {}",
                            field.path
                        ),
                    ));
                }
            }
        }
        let mut by_x = BTreeMap::<String, (Value, Group)>::new();
        for &input in &group.inputs {
            let value = frame.descriptions[&input]
                .parameters
                .get(x)
                .cloned()
                .unwrap_or(Value::Null);
            let entry = by_x.entry(store::encode(&value)?).or_insert_with(|| {
                let mut g = group.clone();
                g.id = id(&(&group.id, &value));
                g.inputs.clear();
                (value, g)
            });
            entry.1.inputs.insert(input);
        }
        for revision in revisions {
            let mut points = Vec::new();
            for (value, g) in by_x.values() {
                let eligible = membership(g, revisions, data, metric, plan.coverage);
                let mut p = point(g, revision, &eligible, data, metric, plan);
                p.x = value.clone();
                points.push(p);
            }
            points.sort_by(|a, b| match (a.x.as_f64(), b.x.as_f64()) {
                (Some(a), Some(b)) => a.total_cmp(&b),
                _ => a.x.to_string().cmp(&b.x.to_string()),
            });
            let mut dimensions = group.dimensions.clone();
            dimensions.insert("revision".into(), json!(revision));
            series.push(Series {
                id: id(&(&group.id, revision, "scaling")),
                label: format!("{} / {}", group.label, revision),
                dimensions,
                points,
            });
        }
    }
    Ok(series)
}
