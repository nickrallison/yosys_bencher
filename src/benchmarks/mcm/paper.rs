use super::{McmInput, PaperStats, parse};
use crate::experiment::{ExperimentError, RunContext};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, ExperimentError>;
static SESSION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaperToolId(String);

#[derive(Debug, Clone)]
pub struct PaperTool {
    pub executable: PathBuf,
}

impl PaperTool {
    pub fn identity(&self) -> Result<PaperToolId> {
        Ok(PaperToolId(format!(
            "{:x}",
            Sha256::digest(fs::read(crate::tools::executable(&self.executable)?)?)
        )))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperReference {
    pub id: String,
    pub executable: PaperToolId,
    pub measured_at: SystemTime,
    pub os: String,
    pub arch: String,
    pub measurements: PaperStats,
}

crate::benchmark_metrics! {
    PaperReference {
        group measurements(PaperStats);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PaperKey {
    coefficients: Vec<i64>,
    input_width: u32,
    bit_width: u32,
    seed: u64,
    sample_index: u32,
}

/// New for each invocation; shared across that invocation's tested revisions.
pub struct PaperSession {
    tool: PaperTool,
    identity: PaperToolId,
    scope: String,
    references: RefCell<HashMap<PaperKey, PaperReference>>,
}

impl PaperSession {
    pub fn new(tool: &PaperTool) -> Result<Self> {
        let tool = PaperTool {
            executable: crate::tools::executable(&tool.executable)?,
        };
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

    pub(super) fn validate_identity(&self, identity: &PaperToolId) -> Result<()> {
        if identity != &self.identity || self.tool.identity()? != self.identity {
            return Err(ExperimentError::Config(
                "paper executable differs from the workload or changed during the invocation"
                    .into(),
            ));
        }
        Ok(())
    }

    pub(super) fn reference(
        &self,
        input: &McmInput,
        context: &mut RunContext,
    ) -> Result<PaperReference> {
        self.validate_identity(&input.paper_identity)?;
        let key = PaperKey {
            coefficients: input.coefficients.clone(),
            input_width: input.input_width,
            bit_width: input.options.paper_bit_width,
            seed: input.options.paper_seed,
            sample_index: *context.sample_index(),
        };
        if let Some(reference) = self.references.borrow().get(&key) {
            return Ok(reference.clone());
        }
        let measured_at = SystemTime::now();
        let mut command = Command::new(&self.tool.executable);
        command.args(["-b", &input.options.paper_bit_width.to_string()]);
        command.args(input.coefficients.iter().map(ToString::to_string));
        command.args(["-gc", "-seed", &input.options.paper_seed.to_string()]);
        let output = context.run_process("paper", &mut command)?;
        let measurements = parse::paper(output.stdout()?, output.elapsed)?;
        let mut references = self.references.borrow_mut();
        let reference = PaperReference {
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
