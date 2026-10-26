use std::{
    cell::RefCell,
    hash::Hash,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use contracts::contract_trait;
use serde::{Deserialize, Serialize};

use super::{BenchmarkOut, ExperimentError, RunContext};
use crate::{
    metrics::{MetricDescriptor, MetricValue},
    tools::{
        external::{ExecutableId, ExternalTool},
        perf::PerfMeasurements,
    },
};

type Result<T> = std::result::Result<T, ExperimentError>;
mod cache;
pub use cache::{ReferenceCache, ReferenceMetadata, ReferenceObservation};

static SESSION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference<M> {
    pub id: String,
    pub executable: ExecutableId,
    pub measured_at: SystemTime,
    pub os: String,
    pub arch: String,
    pub measurements: M,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perf: Option<PerfMeasurements>,
}

#[contract_trait]
impl<M: BenchmarkOut> BenchmarkOut for Reference<M> {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::group("measurements", M::list_metrics()),
            MetricDescriptor::group("perf", PerfMeasurements::list_metrics()),
        ]
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "measurements" => Some(self.measurements.metrics()),
            "perf" => self.perf.as_ref().map(BenchmarkOut::metrics),
            _ => None,
        }
    }
}

pub struct ReferenceSession<K, M> {
    tool: ExternalTool,
    cache: RefCell<ReferenceCache<K, M>>,
}

impl<K: Eq + Hash, M: Clone> ReferenceSession<K, M> {
    pub fn new(tool: &ExternalTool) -> Result<Self> {
        let tool = tool.resolved()?;
        let metadata = ReferenceMetadata {
            executable: tool.identity()?,
            scope: format!(
                "{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                SESSION.fetch_add(1, Ordering::Relaxed)
            ),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
        };
        Ok(Self::from_parts(tool, metadata))
    }

    pub fn from_parts(tool: ExternalTool, metadata: ReferenceMetadata) -> Self {
        Self {
            tool,
            cache: RefCell::new(ReferenceCache::new(metadata)),
        }
    }

    pub fn identity(&self) -> ExecutableId {
        self.cache.borrow().identity().clone()
    }

    pub fn validate_identity(&self, identity: &ExecutableId) -> Result<()> {
        let current = self.tool.identity()?;
        self.cache.borrow().validate_identity(identity, &current)
    }

    pub fn measure(
        &self,
        identity: &ExecutableId,
        key: K,
        context: &mut RunContext,
        run: impl FnOnce(&ExternalTool, &mut RunContext) -> Result<(M, Option<PerfMeasurements>)>,
    ) -> Result<Reference<M>> {
        self.validate_identity(identity)?;
        let key = (key, *context.sample_index());
        if let Some(reference) = self.cache.borrow().get(&key) {
            return Ok(reference.clone());
        }
        let measured_at = SystemTime::now();
        let observation =
            run(&self.tool, context).map(|(measurements, perf)| ReferenceObservation {
                measured_at,
                measurements,
                perf,
            });
        self.cache.borrow_mut().record(key, observation)
    }
}
