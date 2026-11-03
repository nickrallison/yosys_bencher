use super::*;

pub(super) fn coverage<I: BenchmarkIn, O: BenchmarkOut>(
    frame: &ResultFrame<I, O>,
    plan: &AnalysisPlan,
    metric: Option<&MetricSpec>,
    metrics: &[MetricSpec],
    cache: &mut BTreeMap<AttemptId, Sample>,
) -> Vec<CoverageCell> {
    let partitions: BTreeSet<_> = frame.runs.iter().map(|r| r.partition.clone()).collect();
    let partitions = if partitions.is_empty() {
        BTreeSet::from(["unmeasured".into()])
    } else {
        partitions
    };
    let mut cells = Vec::new();
    for partition in partitions {
        for revision in &frame.revisions {
            for &input in frame.inputs.keys() {
                let works: Vec<_> = frame
                    .work_items
                    .iter()
                    .filter(|w| {
                        w.input == input
                            && w.revision == *revision.id().id()
                            && frame
                                .runs
                                .iter()
                                .any(|r| r.id == w.run && r.partition == partition)
                    })
                    .collect();
                let newer_unfinished = frame
                    .newer_unfinished
                    .iter()
                    .filter(|w| {
                        w.input == input
                            && w.revision == *revision.id().id()
                            && frame
                                .runs
                                .iter()
                                .any(|r| r.id == w.run && r.partition == partition)
                    })
                    .map(|w| w.id)
                    .collect::<Vec<_>>();
                if works.is_empty() {
                    cells.push(CoverageCell {
                        id: id(&("coverage", &partition, revision.id().id(), input)),
                        revision: revision.id().id().clone(),
                        input,
                        partition: partition.clone(),
                        run: None,
                        state: None,
                        reason: Some("Unmeasured in selected runs".into()),
                        planned_repetitions: 0,
                        outcomes: OutcomeCounts::default(),
                        work_items: vec![],
                        newer_unfinished,
                    });
                    continue;
                }
                for work in works {
                    let mut outcomes = OutcomeCounts::default();
                    let attempts: Vec<_> = frame
                        .attempts
                        .iter()
                        .filter(|a| a.work_item == work.id)
                        .collect();
                    outcomes.missing =
                        (work.planned_repetitions as usize).saturating_sub(attempts.len());
                    for attempt in attempts {
                        match attempt.record.as_ref().map(|r| &r.outcome) {
                            Some(AttemptOutcome::Success(_)) => outcomes.success += 1,
                            Some(AttemptOutcome::Failure(_)) => outcomes.failure += 1,
                            Some(AttemptOutcome::Unsupported(_)) => outcomes.unsupported += 1,
                            None => outcomes.unfinished += 1,
                        }
                        if let Some(metric) = metric {
                            let s = sample(attempt, work, metric, metrics, plan, cache);
                            if let Some(reason) = s.observation.reason {
                                if reason.starts_with("Missing")
                                    || reason.starts_with("Metric outside")
                                    || reason.starts_with("Invalid formula")
                                {
                                    outcomes.missing_metric += 1;
                                }
                                if reason.starts_with("Excluded") {
                                    outcomes.excluded += 1;
                                }
                            }
                        }
                    }
                    cells.push(CoverageCell {
                        id: id(&("coverage", work.id)),
                        revision: work.revision.clone(),
                        input,
                        partition: partition.clone(),
                        run: Some(work.run),
                        state: Some(work.state),
                        reason: work.reason.clone(),
                        planned_repetitions: work.planned_repetitions as usize,
                        outcomes,
                        work_items: vec![work.id],
                        newer_unfinished: newer_unfinished.clone(),
                    });
                }
            }
        }
    }
    cells
}
