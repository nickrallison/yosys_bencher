use super::PaperReference;
use crate::{
    benchmarks::synth::{SynthMeasurements, SynthOutput, YosysBenchmarkOutput},
    experiment::{BenchmarkOut, ExperimentError},
    metrics::{MetricDescriptor, MetricType, MetricValue},
    tools::yosys::Statistics,
};
use contracts::contract_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

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
        let mut metrics = SynthMeasurements::list_metrics();
        metrics.extend(McmMeasurements::list_metrics());
        metrics
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        self.synthesis
            .get_metric(name)
            .or_else(|| self.mcm.get_metric(name))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmMeasurements {
    /// Yosys pass self CPU time, excluding subprocess startup and other passes.
    pub cpu_time: Duration,
    pub adds_subs: u64,
    pub negations: u64,
    pub multipliers: u64,
    pub depth: Option<u32>,
}

impl McmMeasurements {
    pub fn parse(text: &str, top: &str, cpu_time: Duration) -> Result<Self, ExperimentError> {
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
            cpu_time,
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
                "cpu_time",
                "MCM pass self CPU time",
                MetricType::Duration,
                "s",
                true,
            ),
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
            "cpu_time" => Some(MetricValue::Duration(self.cpu_time)),
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

fn area<'a>(samples: impl Iterator<Item = &'a SynthMeasurements>) -> Option<f64> {
    let areas: Option<Vec<_>> = samples.map(|sample| *sample.area()).collect();
    mean(areas?.into_iter())
}

#[contract_trait]
impl BenchmarkOut for McmOutput {
    /// Scalar accessors summarize repetitions; the original observations remain above.
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::new(
                "mcm.cpu_time",
                "Mean MCM pass self CPU time",
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
                "optimized.runtime",
                "Mean wall time for MCM and synthesis",
                MetricType::Duration,
                "s",
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
        let cpu_time = || {
            mean(
                self.optimized
                    .iter()
                    .map(|s| s.measurements().mcm.cpu_time.as_secs_f64()),
            )
        };
        let reference = || mean(self.paper.samples.iter().map(|s| s.runtime.as_secs_f64()));
        match name {
            "mcm.cpu_time" => cpu_time().map(|v| MetricValue::Duration(Duration::from_secs_f64(v))),
            "paper.runtime" => {
                reference().map(|v| MetricValue::Duration(Duration::from_secs_f64(v)))
            }
            "optimized.runtime" => mean(self.optimized.iter().map(|s| s.runtime().as_secs_f64()))
                .map(|v| MetricValue::Duration(Duration::from_secs_f64(v))),
            "baseline.area" => {
                area(self.baseline.iter().map(|s| s.measurements())).map(MetricValue::Float)
            }
            "optimized.area" => area(self.optimized.iter().map(|s| &s.measurements().synthesis))
                .map(MetricValue::Float),
            "area_savings" => {
                let baseline = area(self.baseline.iter().map(|s| s.measurements()))?;
                if baseline <= 0.0 {
                    return None;
                }
                Some(MetricValue::Float(
                    100.0
                        * (1.0
                            - area(self.optimized.iter().map(|s| &s.measurements().synthesis))?
                                / baseline),
                ))
            }
            "mcm.adds_subs" => mean(
                self.optimized
                    .iter()
                    .map(|s| s.measurements().mcm.adds_subs as f64),
            )
            .map(MetricValue::Float),
            "mcm.multipliers" => mean(
                self.optimized
                    .iter()
                    .map(|s| s.measurements().mcm.multipliers as f64),
            )
            .map(MetricValue::Float),
            "mcm.depth" => {
                let values: Option<Vec<_>> = self
                    .optimized
                    .iter()
                    .map(|s| s.measurements().mcm.depth.map(f64::from))
                    .collect();
                mean(values?.into_iter()).map(MetricValue::Float)
            }
            _ => None,
        }
    }
}
