use super::*;

fn valid_filter(filter: &Filter, field: &str, kind: FieldKind) -> Result<()> {
    filter
        .validate(&[InputField::new(field, field, kind)])
        .map_err(|e| invalid("filters", e))
}

pub(crate) fn validate<I: BenchmarkIn>(
    plan: &AnalysisPlan,
    metrics: &[MetricSpec],
) -> Result<Option<MetricSpec>> {
    if plan.paging.limit > 10000 {
        return Err(invalid("paging.limit", "maximum is 10000"));
    }
    for dimension in &plan.series {
        if let Dimension::Parameter(path) = dimension
            && !I::fields().iter().any(|f| f.path == *path && f.groupable)
        {
            return Err(invalid("series", format!("unknown parameter {path}")));
        }
    }
    for f in &plan.filters {
        match f.stage {
            FilterStage::Attempt => valid_filter(&f.filter, "outcome", FieldKind::Text)?,
            FilterStage::Input => valid_filter(&f.filter, "value", FieldKind::Number)?,
        }
    }
    for filter in &plan.display_filters {
        valid_filter(filter, "value", FieldKind::Number)?;
    }
    if !matches!(plan.selection.runs, RunSelection::LatestCompleted)
        && plan.runs.is_none()
        && !matches!(
            plan.operation,
            Operation::Coverage | Operation::Variation { .. }
        )
    {
        return Err(invalid(
            "runs",
            "choose separate runs, per-run summary reduction, or explicit pooling",
        ));
    }
    let check_threshold = |t: Threshold| {
        if !t.value.is_finite() || t.value < 0.0 || t.kind == ChangeKind::Ratio {
            Err(invalid(
                "threshold",
                "use a nonnegative absolute or percent threshold",
            ))
        } else {
            Ok(())
        }
    };
    if let Some(cohort) = &plan.cohort {
        check_threshold(cohort.threshold)?;
    }
    if let Operation::Changes { threshold, .. } = plan.operation {
        check_threshold(threshold)?;
    }
    if plan.normalization.is_some()
        && !matches!(
            plan.operation,
            Operation::History | Operation::Scaling { .. }
        )
    {
        return Err(invalid("normalization", "supported in History and Scaling"));
    }
    if let Operation::Scaling { x, .. } = &plan.operation
        && !I::fields().iter().any(|f| f.path == *x)
    {
        return Err(invalid("operation.x", format!("unknown parameter {x}")));
    }
    if matches!(plan.operation, Operation::Coverage)
        && (!plan.filters.is_empty() || !plan.display_filters.is_empty() || plan.cohort.is_some())
    {
        return Err(invalid(
            "filters",
            "Coverage preserves the selected workload; use dataset predicates",
        ));
    }
    let metric = plan
        .metric
        .as_ref()
        .map(|path| {
            metrics
                .iter()
                .find(|m| m.path == *path)
                .cloned()
                .ok_or_else(|| invalid("metric", format!("unknown metric {path}")))
        })
        .transpose()?;
    if metric.is_none() && !matches!(plan.operation, Operation::Coverage) {
        return Err(invalid("metric", "a metric is required"));
    }
    if let Some(m) = &metric {
        if plan.cohort.is_some() && m.direction == Direction::Unspecified {
            return Err(invalid(
                "cohort",
                "metric improvement direction is unspecified",
            ));
        }

        if !m.repetitions.contains(&plan.repetitions) {
            return Err(invalid("repetitions", "unsupported metric reduction"));
        }
        if !m.inputs.contains(&plan.inputs)
            && !(plan
                .normalization
                .as_ref()
                .is_some_and(|n| n.kind == ChangeKind::Ratio)
                && plan.inputs == Reduction::GeometricMean)
        {
            return Err(invalid("inputs", "unsupported metric reduction"));
        }
        if let Some(RunTreatment::Summaries { reduction }) = plan.runs
            && !m.runs.contains(&reduction)
        {
            return Err(invalid("runs", "unsupported metric reduction"));
        }
        if let MetricSource::Formula {
            numerator,
            denominator,
            stage,
            ..
        } = &m.source
        {
            for operand in [numerator, denominator] {
                if !metrics
                    .iter()
                    .any(|m| m.path == *operand && matches!(m.source, MetricSource::Path))
                {
                    return Err(invalid(
                        "metric",
                        "formula operands must be declared scalar paths",
                    ));
                }
            }
            if *stage == FormulaStage::Suite
                && (!matches!(
                    plan.operation,
                    Operation::History | Operation::Scaling { .. }
                ) || plan.normalization.is_some()
                    || plan.cohort.is_some()
                    || plan
                        .filters
                        .iter()
                        .any(|f| matches!(f.stage, FilterStage::Input)))
            {
                return Err(invalid(
                    "metric",
                    "suite formulas support unnormalized History or Scaling without value filters or cohorts",
                ));
            }
            if *stage != FormulaStage::Repetition
                && matches!(
                    plan.operation,
                    Operation::Variation {
                        population: Population::WithinRun
                    }
                )
            {
                return Err(invalid(
                    "metric",
                    "within-run distributions require a repetition-level metric",
                ));
            }
        }
    }
    Ok(metric)
}
