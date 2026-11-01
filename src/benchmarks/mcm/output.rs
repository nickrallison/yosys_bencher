use super::PaperReference;
use crate::{
    benchmarks::{
        synth::{SynthMeasurements, SynthOutput},
        yosys::YosysBenchmarkOutput,
    },
    experiment::BenchmarkOut,
    metrics::{MetricDescriptor, MetricType, MetricValue},
};
use contracts::contract_trait;
use serde::{Deserialize, Serialize};

mod mcm;
mod paper;
pub use mcm::McmMeasurements;
pub use paper::PaperStats;

pub type OptimizedOutput = YosysBenchmarkOutput<OptimizedMeasurements>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedMeasurements {
    pub optimization: McmMeasurements,
    pub synthesis: SynthMeasurements,
}

#[contract_trait]
impl BenchmarkOut for OptimizedMeasurements {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::group("synth", SynthMeasurements::list_metrics()),
            MetricDescriptor::group("mcm_stats", McmMeasurements::list_metrics()),
        ]
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "mcm_stats" => Some(self.optimization.metrics()),
            "synth" => Some(self.synthesis.metrics()),
            _ => None,
        }
    }
}

// Expose domain groups without changing the serialized output wrappers.
fn measurement_groups<O: BenchmarkOut>(groups: Vec<MetricDescriptor>) -> Vec<MetricDescriptor> {
    let mut descriptors = O::list_metrics();
    let index = descriptors
        .iter()
        .position(|d| d.name() == "measurements")
        .expect("measurement wrapper");
    descriptors.splice(index..=index, groups);
    descriptors
}

fn measurement_values(
    output: &impl BenchmarkOut,
    groups: impl IntoIterator<Item = (&'static str, MetricValue)>,
) -> MetricValue {
    let MetricValue::Group(mut values) = output.metrics() else {
        unreachable!("output metrics are grouped")
    };
    values.remove("measurements");
    values.extend(groups.into_iter().map(|(name, value)| (name.into(), value)));
    MetricValue::Group(values)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmOutput {
    pub reference: PaperReference,
    pub baseline: SynthOutput,
    pub optimized: OptimizedOutput,
}

impl McmOutput {
    pub fn area_savings(&self) -> Option<f64> {
        let baseline = (*self.baseline.measurements().area())?;
        let optimized = (*self.optimized.measurements().synthesis.area())?;
        (baseline > 0.0).then_some(100.0 * (1.0 - optimized / baseline))
    }
}

#[contract_trait]
impl BenchmarkOut for McmOutput {
    fn observation_identity(&self, path: &str) -> Option<String> {
        path.strip_prefix("reference.")
            .map(|p| format!("reference:{}:{p}", self.reference.id))
    }

    fn analysis_metrics() -> Vec<crate::metrics::MetricSpec> {
        use crate::metrics::*;
        let mut metrics = crate::metrics::describe(&Self::list_metrics());
        for metric in &mut metrics {
            metric.direction = Direction::Lower;
            if metric.path != "area_savings" {
                metric.domain = Domain::Nonnegative;
            }
            if metric.path == "area_savings" {
                metric.direction = Direction::Higher;
                metric.reference = Some(0.0);
                metric.source = MetricSource::Formula {
                    numerator: "optimized.synth.area".into(),
                    denominator: "baseline.synth.area".into(),
                    formula: FormulaKind::PercentSavings,
                    stage: FormulaStage::Input,
                };
                metric.inputs.retain(|r| *r != Reduction::Sum);
            }
            if metric.path.contains(".perf.") {
                metric.direction = Direction::Unspecified;
            }
        }
        let mut ratio = metrics
            .iter()
            .find(|m| m.path == "optimized.runtime")
            .unwrap()
            .clone();
        ratio.path = "optimized_to_reference_runtime".into();
        ratio.label = "Optimized flow / paper runtime".into();
        ratio.description =
            "Complete optimized Yosys flow wall time divided by standalone paper process wall time"
                .into();
        ratio.kind = MetricType::Float;
        ratio.unit = "ratio".into();
        ratio.reference = Some(1.0);
        ratio.source = MetricSource::Formula {
            numerator: "optimized.runtime".into(),
            denominator: "reference.mcm_stats.runtime".into(),
            formula: FormulaKind::Ratio,
            stage: FormulaStage::Input,
        };
        ratio.inputs.retain(|r| *r != Reduction::Sum);
        ratio.inputs.push(Reduction::GeometricMean);
        metrics.push(ratio);
        metrics
    }

    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::group(
                "baseline",
                measurement_groups::<SynthOutput>(vec![MetricDescriptor::group(
                    "synth",
                    SynthMeasurements::list_metrics(),
                )]),
            ),
            MetricDescriptor::group(
                "optimized",
                measurement_groups::<OptimizedOutput>(OptimizedMeasurements::list_metrics()),
            ),
            MetricDescriptor::group(
                "reference",
                measurement_groups::<PaperReference>(vec![MetricDescriptor::group(
                    "mcm_stats",
                    PaperStats::list_metrics(),
                )]),
            ),
            MetricDescriptor::new(
                "area_savings",
                "Area saved against baseline",
                MetricType::Float,
                "%",
                false,
            ),
        ]
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "reference" => Some(measurement_values(
                &self.reference,
                [("mcm_stats", self.reference.measurements.metrics())],
            )),
            "baseline" => Some(measurement_values(
                &self.baseline,
                [("synth", self.baseline.measurements().metrics())],
            )),
            "optimized" => Some(measurement_values(
                &self.optimized,
                [
                    ("synth", self.optimized.measurements().synthesis.metrics()),
                    (
                        "mcm_stats",
                        self.optimized.measurements().optimization.metrics(),
                    ),
                ],
            )),
            "area_savings" => self.area_savings().map(MetricValue::Float),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
