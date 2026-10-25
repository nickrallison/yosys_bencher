use crate::{
    experiment::{BenchmarkOut, ExperimentError},
    metrics::{MetricDescriptor, MetricType, MetricValue},
    tools::yosys::Statistics,
};
use contracts::contract_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

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
