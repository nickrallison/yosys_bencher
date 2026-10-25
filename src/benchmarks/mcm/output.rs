use super::PaperReference;
use crate::{
    benchmarks::synth::{SynthOutput, YosysBenchmarkOutput},
    experiment::{BenchmarkOut, ExperimentError, ProcessOutput},
    metrics::{MetricDescriptor, MetricType, MetricValue},
    tools::yosys::Statistics,
};
use contracts::contract_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmOutput {
    pub mcm: Vec<YosysBenchmarkOutput<McmMeasurements>>,
    pub paper: PaperReference,
    pub baseline: Vec<SynthOutput>,
    pub optimized: Vec<SynthOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmMeasurements {
    pub adds_subs: u64,
    pub negations: u64,
    pub multipliers: u64,
    pub depth: Option<u32>,
}

impl McmMeasurements {
    pub fn parse(output: &ProcessOutput, top: &str) -> Result<Self, ExperimentError> {
        let text = output.stdout()?;
        let stats = Statistics::parse(text, Some(top))?;
        let count = |name: &str| stats.cell_count.get(name).copied().unwrap_or(0);
        let mut depth = None;
        for line in text
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("mcm:"))
        {
            if let Some((_, rest)) = line.split_once("adder(s), depth ") {
                let value = rest
                    .split(',')
                    .next()
                    .unwrap_or(rest)
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| ExperimentError::Parse("malformed MCM depth".into()))?;
                depth = Some(depth.map_or(value, |previous: u32| previous.max(value)));
            }
        }
        Ok(Self {
            adds_subs: count("$add") + count("$sub"),
            negations: count("$neg"),
            multipliers: count("$mul"),
            depth,
        })
    }
}

#[contract_trait]
impl BenchmarkOut for McmMeasurements {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::new(
                "adds_subs",
                "Add/subtract operations",
                MetricType::UInt,
                "operations",
                false,
            ),
            MetricDescriptor::new(
                "negations",
                "Negations",
                MetricType::UInt,
                "operations",
                false,
            ),
            MetricDescriptor::new(
                "multipliers",
                "Remaining multipliers",
                MetricType::UInt,
                "operations",
                false,
            ),
            MetricDescriptor::new("depth", "MCM depth", MetricType::UInt, "levels", false),
        ]
    }
    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "adds_subs" => Some(MetricValue::UInt(self.adds_subs)),
            "negations" => Some(MetricValue::UInt(self.negations)),
            "multipliers" => Some(MetricValue::UInt(self.multipliers)),
            "depth" => self.depth.map(|v| MetricValue::UInt(v.into())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperStats {
    pub runtime: Duration,
    pub adds_subs: u64,
    pub shifts: u64,
    pub negations: u64,
    pub multipliers: u64,
    pub depth: u32,
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, count) = values.fold((0.0, 0_u64), |(sum, count), value| (sum + value, count + 1));
    (count > 0).then(|| sum / count as f64)
}

fn area(samples: &[SynthOutput]) -> Option<f64> {
    let areas: Option<Vec<_>> = samples
        .iter()
        .map(|sample| *sample.measurements().area())
        .collect();
    mean(areas?.into_iter())
}

#[contract_trait]
impl BenchmarkOut for McmOutput {
    /// Scalar accessors summarize repetitions; the original observations remain above.
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::new(
                "mcm.runtime",
                "Mean MCM process runtime",
                MetricType::Duration,
                "s",
                true,
            ),
            MetricDescriptor::new(
                "paper.runtime",
                "Mean paper process runtime",
                MetricType::Duration,
                "s",
                true,
            ),
            MetricDescriptor::new(
                "runtime_ratio",
                "Mean MCM runtime divided by mean paper runtime",
                MetricType::Float,
                "ratio",
                true,
            ),
            MetricDescriptor::new(
                "baseline.area",
                "Mean baseline area",
                MetricType::Float,
                "area",
                false,
            ),
            MetricDescriptor::new(
                "optimized.area",
                "Mean area with MCM",
                MetricType::Float,
                "area",
                false,
            ),
            MetricDescriptor::new(
                "area_savings",
                "Area saved against baseline",
                MetricType::Float,
                "%",
                false,
            ),
            MetricDescriptor::new(
                "mcm.adds_subs",
                "Mean add/subtract operations",
                MetricType::Float,
                "operations",
                false,
            ),
            MetricDescriptor::new(
                "mcm.multipliers",
                "Mean remaining multipliers",
                MetricType::Float,
                "operations",
                false,
            ),
            MetricDescriptor::new(
                "mcm.depth",
                "Mean MCM depth",
                MetricType::Float,
                "levels",
                false,
            ),
        ]
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        let runtime = || mean(self.mcm.iter().map(|s| s.runtime().as_secs_f64()));
        let reference = || mean(self.paper.samples.iter().map(|s| s.runtime.as_secs_f64()));
        match name {
            "mcm.runtime" => runtime().map(|v| MetricValue::Duration(Duration::from_secs_f64(v))),
            "paper.runtime" => {
                reference().map(|v| MetricValue::Duration(Duration::from_secs_f64(v)))
            }
            "runtime_ratio" => {
                let denominator = reference()?;
                if denominator <= 0.0 {
                    return None;
                }
                Some(MetricValue::Float(runtime()? / denominator))
            }
            "baseline.area" => area(&self.baseline).map(MetricValue::Float),
            "optimized.area" => area(&self.optimized).map(MetricValue::Float),
            "area_savings" => {
                let baseline = area(&self.baseline)?;
                if baseline <= 0.0 {
                    return None;
                }
                Some(MetricValue::Float(
                    100.0 * (1.0 - area(&self.optimized)? / baseline),
                ))
            }
            "mcm.adds_subs" => mean(self.mcm.iter().map(|s| s.measurements().adds_subs as f64))
                .map(MetricValue::Float),
            "mcm.multipliers" => mean(self.mcm.iter().map(|s| s.measurements().multipliers as f64))
                .map(MetricValue::Float),
            "mcm.depth" => {
                let values: Option<Vec<_>> = self
                    .mcm
                    .iter()
                    .map(|s| s.measurements().depth.map(f64::from))
                    .collect();
                mean(values?.into_iter()).map(MetricValue::Float)
            }
            _ => None,
        }
    }
}
