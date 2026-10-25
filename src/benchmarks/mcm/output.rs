use super::PaperReference;
use crate::{
    benchmarks::synth::{SynthMeasurements, SynthOutput, YosysBenchmarkOutput},
    experiment::BenchmarkOut,
    metrics::{MetricDescriptor, MetricType, MetricValue},
};
use contracts::contract_trait;
use serde::{Deserialize, Serialize};

mod mcm;
mod paper;
pub use mcm::McmMeasurements;
pub use paper::PaperStats;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmOutput {
    pub paper: PaperReference,
    /// Full synthesis and mapping without MCM.
    pub baseline: Vec<SynthOutput>,
    /// MCM statistics and mapped synthesis from the same Yosys invocation.
    pub optimized: Vec<OptimizedOutput>,
}

pub type OptimizedOutput = YosysBenchmarkOutput<OptimizedMeasurements>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedMeasurements {
    pub mcm: McmMeasurements,
    pub synthesis: SynthMeasurements,
}

#[contract_trait]
impl BenchmarkOut for OptimizedMeasurements {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::group("mcm", McmMeasurements::list_metrics()),
            MetricDescriptor::group("synthesis", SynthMeasurements::list_metrics()),
        ]
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "mcm" => Some(self.mcm.metrics()),
            "synthesis" => Some(self.synthesis.metrics()),
            _ => None,
        }
    }
}

#[contract_trait]
impl BenchmarkOut for McmOutput {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::group("paper", PaperReference::list_metrics()),
            MetricDescriptor::samples("baseline", SynthOutput::list_metrics()),
            MetricDescriptor::samples("optimized", OptimizedOutput::list_metrics()),
            MetricDescriptor::new(
                "area_savings",
                "Area saved against mean baseline area",
                MetricType::Float,
                "%",
                false,
            ),
        ]
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "paper" => Some(self.paper.metrics()),
            "baseline" => Some(MetricValue::Samples(
                self.baseline.iter().map(BenchmarkOut::metrics).collect(),
            )),
            "optimized" => Some(MetricValue::Samples(
                self.optimized.iter().map(BenchmarkOut::metrics).collect(),
            )),
            "area_savings" => {
                let baseline = mean_area(self.baseline.iter().map(|s| s.measurements()))?;
                let optimized =
                    mean_area(self.optimized.iter().map(|s| &s.measurements().synthesis))?;
                (baseline > 0.0).then_some(MetricValue::Float(100.0 * (1.0 - optimized / baseline)))
            }
            _ => None,
        }
    }
}

fn mean_area<'a>(samples: impl Iterator<Item = &'a SynthMeasurements>) -> Option<f64> {
    let mut samples = samples;
    let (sum, count) = samples.try_fold((0.0, 0_u64), |(sum, count), sample| {
        Some((sum + (*sample.area())?, count + 1))
    })?;
    (count > 0).then(|| sum / count as f64)
}
