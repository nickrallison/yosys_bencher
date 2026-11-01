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
