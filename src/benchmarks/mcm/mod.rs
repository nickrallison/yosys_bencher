use crate::{
    benchmarks::synth::{CellLibrary, SynthConfig, comparison::compare},
    experiment::{BenchmarkIn, ExperimentError, Procedure, RunContext, Suite},
    tools::yosys::Yosys,
};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

mod coefficients;
mod design;
mod output;
mod paper;
mod parse;

pub use coefficients::{CoefficientGenerator, CoefficientSets};
pub use output::{McmMeasurements, McmOutput, OptimizedMeasurements, OptimizedOutput, PaperStats};
pub use paper::{PaperReference, PaperSession, PaperTool, PaperToolId};

type Result<T> = std::result::Result<T, ExperimentError>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct McmOptions {
    pub max_depth: u32,
    pub search_budget: u64,
    pub paper_seed: u64,
    pub paper_bit_width: u32,
}

impl Default for McmOptions {
    fn default() -> Self {
        Self {
            max_depth: 16,
            search_budget: 200_000_000,
            paper_seed: 1,
            paper_bit_width: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct McmInput {
    pub coefficients: Vec<i64>,
    pub input_width: u32,
    pub options: McmOptions,
    pub synthesis: SynthConfig,
    pub paper_identity: PaperToolId,
}

impl BenchmarkIn for McmInput {}

impl McmInput {
    pub fn validate(&self) -> Result<()> {
        let invalid = |message: &str| ExperimentError::Config(message.into());
        if self.coefficients.is_empty() {
            return Err(invalid("coefficient set is empty"));
        }
        if self.input_width == 0 || self.input_width > i32::MAX as u32 - 33 {
            return Err(invalid(
                "input width is outside the supported signed Verilog range",
            ));
        }
        if !(1..=30).contains(&self.options.paper_bit_width) {
            return Err(invalid("paper bit width must be between 1 and 30"));
        }
        if self.options.paper_seed > i32::MAX as u64 {
            return Err(invalid(
                "paper seed exceeds its signed 32-bit command-line range",
            ));
        }
        if self.options.max_depth == 0
            || self.options.max_depth > i32::MAX as u32
            || self.options.search_budget == 0
            || self.options.search_budget > i64::MAX as u64
        {
            return Err(invalid(
                "MCM depth and search budget must be positive signed integers",
            ));
        }
        for &coefficient in &self.coefficients {
            if coefficient == 0 {
                return Err(ExperimentError::Unsupported(
                    "the paper tool does not support zero coefficients".into(),
                ));
            }
            if i32::try_from(coefficient).is_err()
                || coefficient.unsigned_abs() >= (1u64 << (self.options.paper_bit_width + 1))
            {
                return Err(invalid(
                    "coefficient exceeds the paper tool's integer/search range",
                ));
            }
        }
        Ok(())
    }
}

pub struct McmTools {
    pub yosys: Yosys,
    pub paper: Rc<PaperSession>,
}

pub struct McmProcedure;

impl Procedure for McmProcedure {
    type Input = McmInput;
    type Tools = McmTools;
    type Output = McmOutput;

    fn name(&self) -> &str {
        "mcm"
    }

    fn run(
        &self,
        input: &McmInput,
        tools: &McmTools,
        context: &mut RunContext,
    ) -> Result<McmOutput> {
        input.validate()?;
        tools.paper.validate_identity(&input.paper_identity)?;
        let design = design::build(input)?;
        compare(
            &tools.yosys,
            &design,
            &input.synthesis,
            context,
            |job| {
                job.stage_with_statistics(
                    "mcm",
                    vec![format!(
                        "mcm -depth {} -search_budget {}",
                        input.options.max_depth, input.options.search_budget,
                    )],
                )
            },
            McmMeasurements::parse_run,
            |context| tools.paper.reference(input, context),
        )
    }
}

pub struct McmSuite {
    pub name: String,
    pub coefficient_sets: CoefficientSets,
    pub input_width: u32,
    pub library: CellLibrary,
    pub paper: PaperTool,
    pub options: McmOptions,
    procedure: McmProcedure,
}

impl McmSuite {
    pub fn new(coefficient_sets: CoefficientSets, library: CellLibrary, paper: PaperTool) -> Self {
        Self {
            name: "mcm".into(),
            coefficient_sets,
            input_width: 32,
            library,
            paper,
            options: McmOptions::default(),
            procedure: McmProcedure,
        }
    }
}

impl Suite for McmSuite {
    type Tool = Yosys;
    type Session = Rc<PaperSession>;
    type Procedure = McmProcedure;

    fn name(&self) -> &str {
        &self.name
    }
    fn procedure(&self) -> &McmProcedure {
        &self.procedure
    }
    fn inputs(&self, session: &Rc<PaperSession>) -> Result<Vec<McmInput>> {
        let identity = session.identity();
        Ok(self
            .coefficient_sets
            .generate()?
            .into_iter()
            .map(|coefficients| McmInput {
                coefficients,
                input_width: self.input_width,
                options: self.options.clone(),
                synthesis: SynthConfig::new(self.library.clone(), Vec::new()),
                paper_identity: identity.clone(),
            })
            .collect())
    }
    fn begin(&self) -> Result<Rc<PaperSession>> {
        Ok(Rc::new(PaperSession::new(&self.paper)?))
    }
    fn tools(&self, tool: &Yosys, session: &Rc<PaperSession>) -> Result<McmTools> {
        Ok(McmTools {
            yosys: tool.clone(),
            paper: Rc::clone(session),
        })
    }
}
