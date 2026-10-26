//! MCM and baseline synthesis comparison with a paper reference.

use crate::{
    benchmarks::{
        synth::{CellLibrary, SynthConfig, SynthFlow, SynthMeasurements},
        yosys::measure,
    },
    experiment::{BenchmarkIn, ExperimentError, Procedure, RunContext, Suite},
    tools::yosys::{Yosys, YosysJob, YosysRunner, design::Design, hierarchy_commands},
};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

mod design;
mod generate;
mod output;
mod paper;
mod parse;

pub use generate::{CoefficientGenerator, CoefficientSets};
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
        let runner = YosysRunner::new(&tools.yosys)?;
        let baseline = runner.prepare(&SynthFlow::job(&design, &input.synthesis)?, context)?;
        let optimized = runner.prepare(&optimized_job(input, design)?, context)?;
        let optimized_output = measure(
            &runner,
            &optimized,
            "optimized",
            context,
            OptimizedMeasurements::parse,
        )
        .map_err(mcm_error)?;
        let paper = tools.paper.reference(input, context)?;
        let baseline_output = measure(
            &runner,
            &baseline,
            "baseline",
            context,
            SynthMeasurements::parse,
        )?;
        for (phase, job) in [("baseline", &baseline), ("optimized", &optimized)] {
            runner.diagnose(job, phase, context);
        }
        Ok(McmOutput {
            paper,
            baseline: baseline_output,
            optimized: optimized_output,
        })
    }
}

fn optimized_job(input: &McmInput, design: Design) -> Result<YosysJob> {
    let mut job = YosysJob {
        design,
        timings: true,
        ..YosysJob::default()
    };
    job.stage("hierarchy", hierarchy_commands(&job.design)?)?;
    job.stage(
        "mcm",
        vec![format!(
            "mcm -depth {} -search_budget {}",
            input.options.max_depth, input.options.search_budget
        )],
    )?;
    job.stage("mcm-stat", vec!["stat -json".into()])?;
    SynthFlow::append(&mut job, &input.synthesis)?;
    Ok(job)
}

fn mcm_error(error: ExperimentError) -> ExperimentError {
    if let ExperimentError::Process { stdout, stderr, .. } = &error
        && [stdout, stderr]
            .iter()
            .any(|bytes| String::from_utf8_lossy(bytes).contains("No such command: mcm"))
    {
        return ExperimentError::Unsupported(
            "this Yosys revision does not provide the mcm pass".into(),
        );
    }
    error
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
    fn inputs(&self) -> Result<Vec<McmInput>> {
        let identity = self.paper.identity()?;
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

#[cfg(test)]
mod tests;
