use std::hash::Hash;

use contracts::{contract_trait, debug_ensures};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::metrics::{MetricDescriptor, MetricValue};

use super::{ExperimentError, RunContext, Version};

/// Procedure name paired with the version resolved from suite comparison.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct ProcedureId {
    name: String,
    version: Version,
}

impl ProcedureId {
    pub fn new(name: impl Into<String>, version: Version) -> Self {
        Self {
            name: name.into(),
            version,
        }
    }
}

/// Measures one workload. Equal inputs may produce different outputs or outcomes.
pub trait Procedure {
    type Input: BenchmarkIn;
    type Tools;
    type Output: BenchmarkOut;

    /// Stable procedure name. The suite resolves its version from previous definitions.
    fn name(&self) -> &str;

    fn run(
        &self,
        input: &Self::Input,
        tools: &Self::Tools,
        context: &mut RunContext,
    ) -> Result<Self::Output, ExperimentError>;
}

/// Workload identity, excluding the tested revision and runtime tool paths.
/// Fixed dependencies must identify their contents, not just their locations.
pub trait BenchmarkIn: Clone + Eq + Hash + Serialize + DeserializeOwned {}

impl BenchmarkIn for () {}

#[contract_trait]
pub trait BenchmarkOut: Clone + Serialize + DeserializeOwned {
    fn list_metrics() -> Vec<MetricDescriptor>;

    /// Metric values with the same hierarchy as `list_metrics`, preserving samples.
    fn metrics(&self) -> MetricValue {
        MetricValue::Group(
            Self::list_metrics()
                .iter()
                .filter_map(|descriptor| {
                    self.get_metric(descriptor.name())
                        .map(|value| (descriptor.name().into(), value))
                })
                .collect(),
        )
    }

    #[debug_ensures(ret.as_ref().is_none_or(|value| {
        Self::list_metrics().iter().any(|descriptor| {
            descriptor.name() == name && descriptor.matches_value(value)
        })
    }))]
    fn get_metric(&self, name: &str) -> Option<MetricValue>;
}

#[contract_trait]
impl BenchmarkOut for () {
    fn list_metrics() -> Vec<MetricDescriptor> {
        Vec::new()
    }

    fn get_metric(&self, _name: &str) -> Option<MetricValue> {
        None
    }
}
