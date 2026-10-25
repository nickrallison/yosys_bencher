use crate::{
    experiment::BenchmarkOut,
    metrics::{MetricDescriptor, MetricType, MetricValue},
};
use contracts::contract_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperStats {
    pub runtime: Duration,
    pub adds_subs: u64,
    pub shifts: u64,
    pub negations: u64,
    pub multipliers: u64,
    pub depth: u32,
}

#[contract_trait]
impl BenchmarkOut for PaperStats {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::new(
                "runtime",
                "Paper process runtime",
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
            MetricDescriptor::new("shifts", "Shifts", MetricType::UInt, "operations", false),
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
            MetricDescriptor::new("depth", "Paper depth", MetricType::UInt, "levels", false),
        ]
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "runtime" => Some(MetricValue::Duration(self.runtime)),
            "adds_subs" => Some(MetricValue::UInt(self.adds_subs)),
            "shifts" => Some(MetricValue::UInt(self.shifts)),
            "negations" => Some(MetricValue::UInt(self.negations)),
            "multipliers" => Some(MetricValue::UInt(self.multipliers)),
            "depth" => Some(MetricValue::UInt(self.depth.into())),
            _ => None,
        }
    }
}
