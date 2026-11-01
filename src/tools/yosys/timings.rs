use super::*;
use crate::{experiment::BenchmarkOut, metrics::*};
use serde::{Deserialize, Serialize};

/// Yosys's internal performance report. Pass runtimes exclude nested passes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct YosysTimings {
    pub generator: String,
    pub total: Duration,
    pub passes: BTreeMap<String, PassTiming>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassTiming {
    pub runtime: Duration,
    pub calls: u64,
}

impl YosysTimings {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        #[derive(Deserialize)]
        struct Pass {
            runtime_ns: u64,
            num_calls: u64,
        }
        #[derive(Deserialize)]
        struct Report {
            generator: String,
            total_ns: u64,
            passes: BTreeMap<String, Pass>,
        }
        let report: Report = serde_json::from_slice(bytes).map_err(|error| {
            ExperimentError::Parse(format!("invalid Yosys pass timings: {error}"))
        })?;
        Ok(Self {
            generator: report.generator,
            total: Duration::from_nanos(report.total_ns),
            passes: report
                .passes
                .into_iter()
                .map(|(name, pass)| {
                    (
                        name,
                        PassTiming {
                            runtime: Duration::from_nanos(pass.runtime_ns),
                            calls: pass.num_calls,
                        },
                    )
                })
                .collect(),
        })
    }
}

#[contracts::contract_trait]
impl BenchmarkOut for PassTiming {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::new(
                "runtime",
                "Yosys pass self CPU time, excluding nested passes",
                MetricType::Duration,
                "s",
                true,
            )
            .with_direction(Direction::Lower),
            MetricDescriptor::new(
                "calls",
                "Number of Yosys pass invocations",
                MetricType::UInt,
                "calls",
                false,
            ),
        ]
    }
    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "runtime" => Some(MetricValue::Duration(self.runtime)),
            "calls" => Some(MetricValue::UInt(self.calls)),
            _ => None,
        }
    }
}

#[contracts::contract_trait]
impl BenchmarkOut for YosysTimings {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::new(
                "total",
                "Total CPU time reported by Yosys's internal pass timers",
                MetricType::Duration,
                "s",
                true,
            )
            .with_direction(Direction::Lower),
            MetricDescriptor::map("passes", PassTiming::list_metrics()),
        ]
    }
    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "total" => Some(MetricValue::Duration(self.total)),
            "passes" => Some(MetricValue::Group(
                self.passes
                    .iter()
                    .map(|(name, pass)| {
                        // Map keys must remain single metric path segments, including plugin pass names.
                        (name.replace('%', "%25").replace('.', "%2E"), pass.metrics())
                    })
                    .collect(),
            )),
            _ => None,
        }
    }
}
