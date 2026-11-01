use super::*;
use crate::{experiment::BenchmarkOut, metrics::*};
use serde::{Deserialize, Serialize};

/// Aggregate self CPU timings and, when available, the ordered invocation tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct YosysTimings {
    pub generator: String,
    pub total: Duration,
    pub passes: BTreeMap<String, PassTiming>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calls: Option<Vec<PassCall>>,
}

/// One invocation. Siblings and children retain execution order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassCall {
    pub name: String,
    /// Inclusive CPU time, including all descendants.
    pub runtime: Duration,
    pub self_runtime: Duration,
    pub children: Vec<PassCall>,
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
            calls: None,
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

    pub fn with_call_trace(mut self, bytes: &[u8]) -> Result<Self> {
        #[derive(Deserialize)]
        struct Call {
            name: String,
            parent: Option<usize>,
            runtime_ns: u64,
        }
        #[derive(Deserialize)]
        struct Trace {
            calls: Vec<Call>,
        }
        let invalid =
            |message| ExperimentError::Parse(format!("invalid Yosys call trace: {message}"));
        let trace: Trace = serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
        let mut stack = Vec::new();
        for (index, call) in trace.calls.iter().enumerate() {
            if let Some(parent) = call.parent {
                while stack.last().is_some_and(|&id| id != parent) {
                    stack.pop();
                }
                if stack.is_empty() {
                    return Err(invalid("parent is not an active ancestor".into()));
                }
            } else {
                stack.clear();
            }
            stack.push(index);
        }
        let mut nodes: Vec<Option<PassCall>> = trace
            .calls
            .iter()
            .map(|call| {
                Some(PassCall {
                    name: call.name.clone(),
                    runtime: Duration::from_nanos(call.runtime_ns),
                    self_runtime: Duration::from_nanos(call.runtime_ns),
                    children: Vec::new(),
                })
            })
            .collect();
        let mut roots = Vec::new();
        for index in (0..nodes.len()).rev() {
            let mut node = nodes[index].take().unwrap();
            node.children.reverse();
            if let Some(parent) = trace.calls[index].parent {
                let parent = nodes[parent].as_mut().unwrap();
                parent.self_runtime = parent
                    .self_runtime
                    .checked_sub(node.runtime)
                    .ok_or_else(|| invalid("children exceed parent runtime".into()))?;
                parent.children.push(node);
            } else {
                roots.push(node);
            }
        }
        roots.reverse();
        fn without_tee(calls: Vec<PassCall>) -> Vec<PassCall> {
            calls
                .into_iter()
                .flat_map(|mut call| {
                    call.children = without_tee(call.children);
                    // The runner inserts tee only to retain stage logs.
                    if call.name == "tee" {
                        call.children
                    } else {
                        call.self_runtime = call.runtime
                            - call
                                .children
                                .iter()
                                .map(|child| child.runtime)
                                .sum::<Duration>();
                        vec![call]
                    }
                })
                .collect()
        }
        self.calls = Some(without_tee(roots));
        Ok(self)
    }
}

fn call_metrics(calls: &[PassCall]) -> MetricValue {
    let mut occurrences = BTreeMap::<&str, usize>::new();
    MetricValue::OrderedGroup(
        calls
            .iter()
            .map(|call| {
                let occurrence = occurrences.entry(&call.name).or_default();
                *occurrence += 1;
                let name = call
                    .name
                    .replace('%', "%25")
                    .replace('.', "%2E")
                    .replace('~', "%7E");
                let key = if *occurrence == 1 {
                    name
                } else {
                    format!("{name}~{occurrence}")
                };
                (
                    key,
                    MetricValue::Group(
                        [
                            ("runtime".into(), MetricValue::Duration(call.runtime)),
                            (
                                "self_runtime".into(),
                                MetricValue::Duration(call.self_runtime),
                            ),
                            ("children".into(), call_metrics(&call.children)),
                        ]
                        .into(),
                    ),
                )
            })
            .collect(),
    )
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
            MetricDescriptor::tree(
                "calls",
                vec![
                    MetricDescriptor::new(
                        "runtime",
                        "Inclusive CPU time for this pass invocation and its subpasses",
                        MetricType::Duration,
                        "s",
                        true,
                    )
                    .with_direction(Direction::Lower),
                    MetricDescriptor::new(
                        "self_runtime",
                        "Self CPU time for this pass invocation, excluding subpasses",
                        MetricType::Duration,
                        "s",
                        true,
                    )
                    .with_direction(Direction::Lower),
                ],
            ),
            MetricDescriptor::map("passes", PassTiming::list_metrics()),
        ]
    }
    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "total" => Some(MetricValue::Duration(self.total)),
            "calls" => self.calls.as_deref().map(call_metrics),
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
