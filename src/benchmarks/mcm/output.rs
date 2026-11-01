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
            MetricDescriptor::group("optimization", McmMeasurements::list_metrics()),
            MetricDescriptor::group("synthesis", SynthMeasurements::list_metrics()),
        ]
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "optimization" => Some(self.optimization.metrics()),
            "synthesis" => Some(self.synthesis.metrics()),
            _ => None,
        }
    }
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
                    numerator: "optimized.measurements.synthesis.area".into(),
                    denominator: "baseline.measurements.area".into(),
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
            denominator: "reference.measurements.runtime".into(),
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
            MetricDescriptor::group("reference", PaperReference::list_metrics()),
            MetricDescriptor::group("baseline", SynthOutput::list_metrics()),
            MetricDescriptor::group("optimized", OptimizedOutput::list_metrics()),
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
            "reference" => Some(self.reference.metrics()),
            "baseline" => Some(self.baseline.metrics()),
            "optimized" => Some(self.optimized.metrics()),
            "area_savings" => self.area_savings().map(MetricValue::Float),
            _ => None,
        }
    }
}
