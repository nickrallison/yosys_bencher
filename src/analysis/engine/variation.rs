use super::*;

fn distribution(
    population: Population,
    revision: String,
    input: Option<InputId>,
    partition: String,
    run: Option<RunId>,
    values: &[f64],
    contributors: Vec<String>,
) -> Distribution {
    Distribution {
        id: id(&(population, &revision, input, &partition, run)),
        population,
        revision,
        input,
        partition,
        run,
        count: values.len(),
        mean: Reduction::Mean.apply(values),
        standard_deviation: Reduction::StdDev.apply(values),
        median: Reduction::Median.apply(values),
        p90: Reduction::P90.apply(values),
        p95: Reduction::P95.apply(values),
        min: Reduction::Min.apply(values),
        max: Reduction::Max.apply(values),
        contributors,
    }
}
pub(super) fn variation(
    groups: &[Group],
    revisions: &[String],
    per_run: &[Estimated],
    data: &BTreeMap<Key, Estimated>,
    population: Population,
    metric: &MetricSpec,
    plan: &AnalysisPlan,
) -> Vec<Distribution> {
    let revision_set: BTreeSet<_> = revisions.iter().collect();
    match population {
        Population::WithinRun => per_run
            .iter()
            .filter(|d| {
                revision_set.contains(&d.estimate.revision)
                    && groups.iter().any(|g| {
                        g.partition == d.estimate.partition && g.inputs.contains(&d.estimate.input)
                    })
            })
            .map(|d| {
                let e = &d.estimate;
                let mut seen = BTreeSet::new();
                let values = if e.value.is_none() {
                    vec![]
                } else {
                    d.samples
                        .iter()
                        .filter(|s| {
                            s.observation.reason.is_none()
                                && seen.insert(s.observation.identities.clone())
                        })
                        .filter_map(|s| s.observation.value)
                        .collect()
                };
                distribution(
                    population,
                    e.revision.clone(),
                    Some(e.input),
                    e.partition.clone(),
                    e.run_group,
                    &values,
                    vec![e.id.clone()],
                )
            })
            .collect(),
        Population::BetweenRuns => {
            let mut keyed = BTreeMap::<(String, String, InputId), Vec<&Estimate>>::new();
            for d in per_run.iter().filter(|d| {
                revision_set.contains(&d.estimate.revision)
                    && groups.iter().any(|g| {
                        g.partition == d.estimate.partition && g.inputs.contains(&d.estimate.input)
                    })
            }) {
                let e = &d.estimate;
                keyed
                    .entry((e.partition.clone(), e.revision.clone(), e.input))
                    .or_default()
                    .push(e);
            }
            keyed
                .into_iter()
                .map(|((partition, revision, input), estimates)| {
                    distribution(
                        population,
                        revision,
                        Some(input),
                        partition,
                        None,
                        &estimates.iter().filter_map(|e| e.value).collect::<Vec<_>>(),
                        estimates.iter().map(|e| e.id.clone()).collect(),
                    )
                })
                .collect()
        }
        Population::AcrossInputs => groups
            .iter()
            .flat_map(|g| {
                let eligible = membership(g, revisions, data, metric, plan.coverage);
                revisions.iter().map(move |r| {
                    let values: Vec<_> = eligible
                        .iter()
                        .filter_map(|&input| {
                            data.get(&(g.partition.clone(), r.clone(), input, g.run))
                        })
                        .collect();
                    let numbers: Vec<_> = values.iter().filter_map(|d| d.estimate.value).collect();
                    let numbers = if plan.coverage == CoveragePolicy::Strict
                        && numbers.len() != eligible.len()
                    {
                        vec![]
                    } else {
                        numbers
                    };
                    let mut d = distribution(
                        population,
                        r.clone(),
                        None,
                        g.partition.clone(),
                        g.run,
                        &numbers,
                        values.iter().map(|d| d.estimate.id.clone()).collect(),
                    );
                    d.id = id(&(&d.id, &g.id));
                    d
                })
            })
            .collect(),
    }
}
