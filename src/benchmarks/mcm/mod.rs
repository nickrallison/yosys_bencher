//! MCM and baseline synthesis comparison with a paper reference.

use crate::{
    benchmarks::synth::{CellLibrary, SynthConfig, SynthFlow, SynthMeasurements, SynthOutput},
    experiment::{BenchmarkIn, ExperimentError, Procedure, ProcedureId, RunContext, Suite, repeat},
    tools::yosys::{
        Yosys, YosysRunner,
        design::{Design, SourceFile},
    },
};
use serde::{Deserialize, Serialize};
use std::{num::NonZeroU32, rc::Rc};

mod design;
mod generate;
mod output;
mod paper;
mod parse;

pub use generate::{CoefficientGenerator, CoefficientSets};
pub use output::{McmMeasurements, McmOutput, OptimizedMeasurements, OptimizedOutput, PaperStats};
pub use paper::{PaperReference, PaperSession, PaperTool, PaperToolId};

pub const VERSION: u32 = 1;
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
    pub repetitions: NonZeroU32,
}

impl BenchmarkIn for McmInput {}

pub struct McmTools {
    pub yosys: Yosys,
    pub paper: Rc<PaperSession>,
}

pub struct McmProcedure;

impl Procedure for McmProcedure {
    type Input = McmInput;
    type Tools = McmTools;
    type Output = McmOutput;

    fn id(&self) -> ProcedureId {
        ProcedureId::new("mcm", VERSION)
    }

    fn run(
        &self,
        input: &McmInput,
        tools: &McmTools,
        context: &mut RunContext,
    ) -> Result<McmOutput> {
        design::validate(input)?;
        tools.paper.validate_identity(&input.paper_identity)?;
        let design = Design {
            files: vec![SourceFile::new("design.v", design::verilog(input)?)],
            top: Some("unoptimized".into()),
            ..Design::default()
        };
        let mcm = format!(
            "mcm -depth {} -search_budget {}",
            input.options.max_depth, input.options.search_budget
        );
        let runner = YosysRunner::new(&tools.yosys)?;
        let baseline = runner.prepare(&SynthFlow::job(&design, &input.synthesis, &[])?, context)?;
        let optimized = runner.prepare(
            &SynthFlow::job(&design, &input.synthesis, &[mcm, "stat".into()])?,
            context,
        )?;
        let optimized_samples = repeat(input.repetitions, |index| {
            let (output, timings) = runner
                .measure_with_timings(&optimized, &format!("optimized-{index}"), context)
                .map_err(mcm_error)?;
            let cpu_time = timings
                .get("mcm")
                .copied()
                .ok_or_else(|| ExperimentError::Parse("missing MCM pass timing".into()))?;
            Ok(OptimizedOutput::new(
                output.elapsed,
                OptimizedMeasurements {
                    mcm: McmMeasurements::parse(
                        &optimized.read_log("before-synth")?,
                        "unoptimized",
                        cpu_time,
                    )?,
                    synthesis: SynthMeasurements::parse(&output, Some("unoptimized"))?,
                },
            ))
        })?;
        let paper = tools.paper.reference(input, context)?;
        let baseline_samples = repeat(input.repetitions, |index| {
            let output = runner.measure(&baseline, &format!("baseline-{index}"), context)?;
            Ok(SynthOutput::new(
                output.elapsed,
                SynthMeasurements::parse(&output, Some("unoptimized"))?,
            ))
        })?;
        for (phase, job) in [("baseline", &baseline), ("optimized", &optimized)] {
            runner.diagnose_repeated(job, phase, input.repetitions, context)?;
        }
        Ok(McmOutput {
            paper,
            baseline: baseline_samples,
            optimized: optimized_samples,
        })
    }
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
    pub repetitions: NonZeroU32,
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
            repetitions: NonZeroU32::new(1).unwrap(),
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
                repetitions: self.repetitions,
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
