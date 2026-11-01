use std::hash::Hash;

use contracts::{contract_trait, debug_ensures};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::metrics::{MetricDescriptor, MetricValue};

use super::{ExperimentError, RunContext, Version};

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

pub trait Procedure {
    type Input: BenchmarkIn;
    type Tools;
    type Output: BenchmarkOut;

    fn name(&self) -> &str;

    fn run(
        &self,
        input: &Self::Input,
        tools: &Self::Tools,
        context: &mut RunContext,
    ) -> Result<Self::Output, ExperimentError>;
}

pub trait BenchmarkIn: Clone + Eq + Hash + Serialize + DeserializeOwned {
    fn fields() -> Vec<super::InputField> {
        Vec::new()
    }
    fn parameters(&self) -> std::collections::BTreeMap<String, serde_json::Value> {
        std::collections::BTreeMap::new()
    }
    fn label(&self) -> String {
        "Input".into()
    }
}

impl BenchmarkIn for () {}

#[contract_trait]
pub trait BenchmarkOut: Clone + Serialize + DeserializeOwned {
    fn list_metrics() -> Vec<MetricDescriptor>;

    fn analysis_metrics() -> Vec<crate::metrics::MetricSpec> {
        crate::metrics::describe(&Self::list_metrics())
    }

    /// Identity within a run for observations reused by multiple outputs.
    fn observation_identity(&self, _path: &str) -> Option<String> {
        None
    }

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
