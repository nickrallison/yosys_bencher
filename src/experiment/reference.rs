//! References measured once per workload and sample within one invocation.

use std::{
    cell::RefCell,
    collections::HashMap,
    hash::Hash,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use contracts::contract_trait;
use serde::{Deserialize, Serialize};

use super::{BenchmarkOut, ExperimentError, RunContext};
use crate::{
    metrics::{MetricDescriptor, MetricValue},
    tools::external::{ExecutableId, ExternalTool},
};

type Result<T> = std::result::Result<T, ExperimentError>;
static SESSION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference<M> {
    pub id: String,
    pub executable: ExecutableId,
    pub measured_at: SystemTime,
    pub os: String,
    pub arch: String,
    pub measurements: M,
}

#[contract_trait]
impl<M: BenchmarkOut> BenchmarkOut for Reference<M> {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![MetricDescriptor::group("measurements", M::list_metrics())]
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        (name == "measurements").then(|| self.measurements.metrics())
    }
}

/// Create in `Suite::begin`, then share across revisions. Generation is never cached.
pub struct ReferenceSession<K, M> {
    tool: ExternalTool,
    identity: ExecutableId,
    scope: String,
    references: RefCell<HashMap<(K, u32), Reference<M>>>,
}

impl<K: Eq + Hash, M: Clone> ReferenceSession<K, M> {
    pub fn new(tool: &ExternalTool) -> Result<Self> {
        let tool = tool.resolved()?;
        Ok(Self {
            identity: tool.identity()?,
            tool,
            scope: format!(
                "{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                SESSION.fetch_add(1, Ordering::Relaxed)
            ),
            references: RefCell::new(HashMap::new()),
        })
    }

    pub fn validate_identity(&self, identity: &ExecutableId) -> Result<()> {
        if identity != &self.identity || self.tool.identity()? != self.identity {
            return Err(ExperimentError::Config(
                "reference executable differs from the workload or changed during the invocation"
                    .into(),
            ));
        }
        Ok(())
    }

    /// The key must include every parameter that affects this reference's execution.
    /// The sample index is included automatically; failures are never cached.
    pub fn measure(
        &self,
        identity: &ExecutableId,
        key: K,
        context: &mut RunContext,
        run: impl FnOnce(&ExternalTool, &mut RunContext) -> Result<M>,
    ) -> Result<Reference<M>> {
        // Recheck even on cache hits: an edited executable invalidates the workload.
        self.validate_identity(identity)?;
        let key = (key, *context.sample_index());
        if let Some(reference) = self.references.borrow().get(&key) {
            return Ok(reference.clone());
        }
        let measured_at = SystemTime::now();
        let measurements = run(&self.tool, context)?;
        let mut references = self.references.borrow_mut();
        let reference = Reference {
            id: format!("{}:{}", self.scope, references.len()),
            executable: self.identity.clone(),
            measured_at,
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            measurements,
        };
        references.insert(key, reference.clone());
        Ok(reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::ArtifactPolicy;
    use std::{cell::Cell, fs};

    #[test]
    fn reference_cache_retries_failures_isolates_samples_and_revalidates_hits() {
        let root = tempfile::tempdir().unwrap();
        let tool = ExternalTool {
            executable: root.path().join("reference"),
        };
        fs::write(&tool.executable, "original executable").unwrap();
        let identity = tool.identity().unwrap();
        let session = ReferenceSession::new(&tool).unwrap();
        let mut context = RunContext::new(
            root.path().join("workspace"),
            ArtifactPolicy::new(false, false, false),
        );
        let calls = Cell::new(0);
        let measure = |_: &ExternalTool, _: &mut RunContext| {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Err(ExperimentError::Parse("temporary failure".into()))
            } else {
                Ok(calls.get())
            }
        };
        assert!(
            session
                .measure(&identity, "workload", &mut context, measure)
                .is_err()
        );
        let first = session
            .measure(&identity, "workload", &mut context, measure)
            .unwrap();
        let cached = session
            .measure(&identity, "workload", &mut context, measure)
            .unwrap();
        assert_eq!(first.id, cached.id);
        assert_eq!(cached.measurements, 2);
        assert_eq!(calls.get(), 2);
        context = context.with_sample_index(1);
        let sample = session
            .measure(&identity, "workload", &mut context, measure)
            .unwrap();
        assert_ne!(first.id, sample.id);
        assert_eq!(sample.measurements, 3);
        let fresh = ReferenceSession::new(&tool)
            .unwrap()
            .measure(&identity, "workload", &mut context, measure)
            .unwrap();
        assert_ne!(sample.id, fresh.id);
        assert_eq!(fresh.measurements, 4);
        fs::write(&tool.executable, "changed executable").unwrap();
        assert!(matches!(
            session.measure(&identity, "workload", &mut context, measure),
            Err(ExperimentError::Config(_))
        ));
        assert_eq!(calls.get(), 4);
    }
}
