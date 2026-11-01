use super::*;

#[derive(Clone)]
pub(super) struct Group {
    pub(super) id: String,
    pub(super) partition: String,
    pub(super) run: Option<RunId>,
    pub(super) label: String,
    pub(super) dimensions: BTreeMap<String, Value>,
    pub(super) inputs: BTreeSet<InputId>,
}
pub(super) fn groups<I: BenchmarkIn, O>(
    frame: &ResultFrame<I, O>,
    plan: &AnalysisPlan,
) -> Result<Vec<Group>> {
    let separate = matches!(plan.runs, Some(RunTreatment::Separate))
        || plan.series.iter().any(|d| matches!(d, Dimension::Run));
    let mut partition_runs: BTreeMap<(String, Option<RunId>), Option<&RunSummary>> =
        BTreeMap::new();
    for run in &frame.runs {
        partition_runs.insert(
            (run.partition.clone(), separate.then_some(run.id)),
            Some(run),
        );
    }
    if partition_runs.is_empty() {
        partition_runs.insert(("unmeasured".into(), None), None);
    }
    let mut groups = BTreeMap::<String, Group>::new();
    for ((partition, run_id), run) in partition_runs {
        for (&input, description) in &frame.descriptions {
            let mut dimensions = BTreeMap::new();
            for d in &plan.series {
                match d {
                    Dimension::Input => {
                        dimensions.insert("input".into(), json!(input));
                    }
                    Dimension::Parameter(path) => {
                        dimensions.insert(
                            path.clone(),
                            description
                                .parameters
                                .get(path)
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                    }
                    Dimension::Run => {
                        dimensions.insert("run".into(), json!(run_id));
                    }
                    Dimension::Environment => {
                        dimensions.insert(
                            "environment".into(),
                            run.map(|r| json!(r.environment)).unwrap_or(Value::Null),
                        );
                    }
                    Dimension::Option(path) => {
                        dimensions.insert(
                            format!("option:{path}"),
                            run.and_then(|r| r.options.pointer(path))
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                    }
                }
            }
            let group_id = id(&(&partition, run_id, &dimensions));
            let label = if dimensions.is_empty() {
                run.map(|r| r.environment.machine.clone())
                    .unwrap_or_else(|| "Unmeasured".into())
            } else {
                dimensions
                    .iter()
                    .map(|(k, v)| {
                        if k == "input" && plan.series.contains(&Dimension::Input) {
                            format!("input: {}", description.label)
                        } else {
                            format!("{k}: {v}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            groups
                .entry(group_id.clone())
                .or_insert_with(|| Group {
                    id: group_id,
                    partition: partition.clone(),
                    run: run_id,
                    label,
                    dimensions,
                    inputs: BTreeSet::new(),
                })
                .inputs
                .insert(input);
        }
    }
    Ok(groups.into_values().collect())
}
