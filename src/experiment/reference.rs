use std::{cell::RefCell, hash::Hash, time::SystemTime};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference<M> {
    pub id: usize,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::ArtifactPolicy;
    use std::{cell::Cell, fs};

    #[test]
    fn sessions_cache_successes_by_workload_and_sample_and_reject_changed_tools() {
        let root = tempfile::tempdir().unwrap();
        let tool = ExternalTool {
            executable: root.path().join("tool"),
        };
        fs::write(&tool.executable, b"original").unwrap();
        let session = ReferenceSession::<_, u32>::new(&tool).unwrap();
        let identity = session.identity();
        let mut context = RunContext::new(
            root.path().join("attempt"),
            ArtifactPolicy::new(false, false, false),
        );
        assert!(
            session
                .measure(&identity, "a", &mut context, |_, _| Err(
                    ExperimentError::Parse("failed observation".into())
                ))
                .is_err()
        );
        let calls = Cell::new(0);
        let measure = |_: &ExternalTool, _: &mut RunContext| {
            calls.set(calls.get() + 1);
            Ok((calls.get(), None))
        };
        let mut first_time = None;
        for (key, sample, expected_id) in [("a", 0, 0), ("a", 0, 0), ("b", 0, 1), ("a", 1, 2)] {
            context = context.with_sample_index(sample);
            let reference = session
                .measure(&identity, key, &mut context, measure)
                .unwrap();
            assert_eq!(reference.id, expected_id);
            assert_eq!(reference.measurements, expected_id as u32 + 1);
            assert_eq!(reference.executable, identity);
            if expected_id == 0 {
                assert_eq!(
                    reference.measured_at,
                    *first_time.get_or_insert(reference.measured_at)
                );
            }
        }
        assert_eq!(calls.get(), 3);
        let fresh = ReferenceSession::<_, u32>::new(&tool).unwrap();
        assert_eq!(
            fresh
                .measure(&identity, "a", &mut context, measure)
                .unwrap()
                .id,
            0
        );
        assert!(
            session
                .validate_identity(&ExecutableId::from_bytes(b"wrong workload"))
                .is_err()
        );
        fs::write(&tool.executable, b"changed").unwrap();
        assert!(matches!(
            session.measure(&identity, "a", &mut context, |_, _| panic!(
                "must not run changed tool"
            )),
            Err(ExperimentError::Config(_))
        ));
    }
}
