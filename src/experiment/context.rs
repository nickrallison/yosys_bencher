use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::tools::perf::PerfProfiler;

/// Runtime resources for one attempt, outside workload identity.
#[derive(Debug, Getters)]
#[getset(get = "pub")]
pub struct RunContext {
    workspace: PathBuf,
    artifacts: ArtifactPolicy,
    perf: Option<PerfProfiler>,
}

impl RunContext {
    pub fn new(workspace: PathBuf, artifacts: ArtifactPolicy) -> Self {
        Self {
            workspace,
            artifacts,
            perf: None,
        }
    }

    pub fn with_perf(mut self, perf: PerfProfiler) -> Self {
        self.perf = Some(perf);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct ArtifactPolicy {
    logs: bool,
    scripts: bool,
    netlists: bool,
}

impl ArtifactPolicy {
    pub fn new(logs: bool, scripts: bool, netlists: bool) -> Self {
        Self {
            logs,
            scripts,
            netlists,
        }
    }
}

impl Default for ArtifactPolicy {
    fn default() -> Self {
        Self::new(true, true, false)
    }
}
