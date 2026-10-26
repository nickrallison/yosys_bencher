use super::{McmInput, PaperStats, parse};
use crate::experiment::{ExperimentError, Reference, ReferenceSession, RunContext};
use std::process::Command;

pub use crate::tools::external::{ExecutableId as PaperToolId, ExternalTool as PaperTool};

pub type PaperReference = Reference<PaperStats>;
type Result<T> = std::result::Result<T, ExperimentError>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PaperKey {
    coefficients: Vec<i64>,
    input_width: u32,
    bit_width: u32,
    seed: u64,
}

pub struct PaperSession(ReferenceSession<PaperKey, PaperStats>);

impl PaperSession {
    pub fn new(tool: &PaperTool) -> Result<Self> {
        Ok(Self(ReferenceSession::new(tool)?))
    }

    pub fn identity(&self) -> PaperToolId {
        self.0.identity()
    }

    pub(super) fn validate_identity(&self, identity: &PaperToolId) -> Result<()> {
        self.0.validate_identity(identity)
    }

    pub(super) fn reference(
        &self,
        input: &McmInput,
        context: &mut RunContext,
    ) -> Result<PaperReference> {
        let key = PaperKey {
            coefficients: input.coefficients.clone(),
            input_width: input.input_width,
            bit_width: input.options.paper_bit_width,
            seed: input.options.paper_seed,
        };
        self.0
            .measure(&input.paper_identity, key, context, |tool, context| {
                let mut command = Command::new(&tool.executable);
                command.args(["-b", &input.options.paper_bit_width.to_string()]);
                command.args(input.coefficients.iter().map(ToString::to_string));
                command.args(["-gc", "-seed", &input.options.paper_seed.to_string()]);
                let output = context.run_process("paper", &mut command)?;
                Ok((parse::paper(output.stdout()?, output.elapsed)?, output.perf))
            })
    }
}
