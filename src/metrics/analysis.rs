use super::{MetricDescriptor, MetricValue};
pub use yosys_bench_ui_types::metrics::*;
pub fn describe(descriptors: &[MetricDescriptor]) -> Vec<MetricSpec> {
    fn visit(out: &mut Vec<MetricSpec>, prefix: &str, descriptors: &[MetricDescriptor]) {
        for descriptor in descriptors {
            let path = if prefix.is_empty() {
                descriptor.name().into()
            } else {
                format!("{prefix}.{}", descriptor.name())
            };
            match descriptor {
                MetricDescriptor::Value {
                    desc,
                    kind,
                    unit,
                    direction,
                    ..
                } => {
                    let mut inputs = Reduction::estimates();
                    inputs.extend([Reduction::Sum, Reduction::Count, Reduction::StdDev]);
                    out.push(MetricSpec {
                        label: path.clone(),
                        path,
                        description: (*desc).into(),
                        kind: kind.clone(),
                        unit: (*unit).into(),
                        direction: *direction,
                        domain: if matches!(kind, MetricType::UInt | MetricType::Duration) {
                            Domain::Nonnegative
                        } else {
                            Domain::Finite
                        },
                        repetitions: Reduction::estimates(),
                        runs: Reduction::estimates(),
                        inputs,
                        default_repetitions: Reduction::Mean,
                        default_runs: Reduction::Mean,
                        default_inputs: Reduction::Mean,
                        reference: None,
                        source: MetricSource::Path,
                    });
                }
                MetricDescriptor::Group { children, .. } => visit(out, &path, children),
                // A nested collection is not a repetition axis. Authors must expose an explicit scalar projection.
                MetricDescriptor::Samples { .. } => (),
                // Named keys are added by describe_dynamic from recorded values.
                MetricDescriptor::Map { .. } | MetricDescriptor::Tree { .. } => (),
            }
        }
    }
    let mut out = Vec::new();
    visit(&mut out, "", descriptors);
    out
}
pub fn scalar(output: &impl crate::experiment::BenchmarkOut, path: &str) -> Option<f64> {
    output.get_scalar(path).filter(|v| v.is_finite())
}
/// Compatibility fallback for outputs without direct scalar access.
pub fn scalar_from_group(output: &impl crate::experiment::BenchmarkOut, path: &str) -> Option<f64> {
    let mut parts = path.split('.');
    let mut value = output.get_metric(parts.next()?)?;
    for part in parts {
        value = match value {
            MetricValue::Group(mut group) => group.remove(part)?,
            MetricValue::OrderedGroup(group) => group.into_iter().find(|(name, _)| name == part)?.1,
            _ => return None,
        };
    }
    let value = match value {
        MetricValue::Int(v) => v as f64,
        MetricValue::UInt(v) => v as f64,
        MetricValue::Float(v) => v,
        MetricValue::Duration(v) => v.as_secs_f64(),
        _ => return None,
    };
    value.is_finite().then_some(value)
}

/// Expand named-record metric maps using keys actually present in an output.
/// Ordinary descriptors keep the procedure's declared analysis metadata.
pub fn describe_dynamic(descriptors: &[MetricDescriptor], value: &MetricValue) -> Vec<MetricSpec> {
    describe_new(descriptors, value, &mut std::collections::BTreeSet::new())
}
pub(crate) fn describe_new(
    descriptors: &[MetricDescriptor],
    value: &MetricValue,
    known: &mut std::collections::BTreeSet<String>,
) -> Vec<MetricSpec> {
    fn visit(
        out: &mut Vec<MetricSpec>,
        prefix: &str,
        descriptors: &[MetricDescriptor],
        value: &MetricValue,
        dynamic: bool,
        known: &mut std::collections::BTreeSet<String>,
    ) {
        let MetricValue::Group(values) = value else {
            return;
        };
        for descriptor in descriptors {
            let Some(value) = values.get(descriptor.name()) else {
                continue;
            };
            let path = if prefix.is_empty() {
                descriptor.name().into()
            } else {
                format!("{prefix}.{}", descriptor.name())
            };
            match descriptor {
                MetricDescriptor::Value { .. } if dynamic && known.insert(path.clone()) => {
                    out.extend(
                        describe(std::slice::from_ref(descriptor))
                            .into_iter()
                            .map(|m| m.prefixed(prefix)),
                    );
                }
                MetricDescriptor::Group { children, .. } => {
                    visit(out, &path, children, value, dynamic, known)
                }
                MetricDescriptor::Map { children, .. } => {
                    if let MetricValue::Group(records) = value {
                        for (name, record) in records {
                            visit(
                                out,
                                &format!("{path}.{name}"),
                                children,
                                record,
                                true,
                                known,
                            );
                        }
                    }
                }
                MetricDescriptor::Tree { children, .. } => {
                    visit_tree(out, &path, children, value, known);
                }
                _ => (),
            }
        }
    }
    fn visit_tree(
        out: &mut Vec<MetricSpec>,
        prefix: &str,
        descriptors: &[MetricDescriptor],
        value: &MetricValue,
        known: &mut std::collections::BTreeSet<String>,
    ) {
        let MetricValue::OrderedGroup(nodes) = value else {
            return;
        };
        for (name, node) in nodes {
            let path = format!("{prefix}.{name}");
            visit(out, &path, descriptors, node, true, known);
            if let MetricValue::Group(fields) = node
                && let Some(children) = fields.get("children")
            {
                visit_tree(
                    out,
                    &format!("{path}.children"),
                    descriptors,
                    children,
                    known,
                );
            }
        }
    }
    let mut out = Vec::new();
    visit(&mut out, "", descriptors, value, false, known);
    out
}

pub fn catalog<'a, O: crate::experiment::BenchmarkOut + 'a>(
    outputs: impl IntoIterator<Item = &'a O>,
) -> Vec<MetricSpec> {
    let mut metrics = O::analysis_metrics();
    let mut known: std::collections::BTreeSet<_> = metrics.iter().map(|m| m.path.clone()).collect();
    let descriptors = O::list_metrics();
    for output in outputs {
        metrics.extend(describe_new(&descriptors, &output.metrics(), &mut known));
    }
    metrics
}
