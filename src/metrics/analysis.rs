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
                    desc, kind, unit, ..
                } => {
                    let mut inputs = Reduction::estimates();
                    inputs.extend([Reduction::Sum, Reduction::Count, Reduction::StdDev]);
                    out.push(MetricSpec {
                        label: path.clone(),
                        path,
                        description: (*desc).into(),
                        kind: kind.clone(),
                        unit: (*unit).into(),
                        direction: Direction::Unspecified,
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
            }
        }
    }
    let mut out = Vec::new();
    visit(&mut out, "", descriptors);
    out
}
pub fn scalar(output: &impl crate::experiment::BenchmarkOut, path: &str) -> Option<f64> {
    let mut parts = path.split('.');
    let mut value = output.get_metric(parts.next()?)?;
    for part in parts {
        value = match value {
            MetricValue::Group(mut group) => group.remove(part)?,
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
