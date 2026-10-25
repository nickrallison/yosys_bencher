//! Four-phase MCM comparison. Execution mechanics live in the shared runners.

use crate::{
    benchmarks::synth::{
        CellLibrary, SynthConfig, SynthFlow, SynthMeasurements, SynthOutput, YosysBenchmarkOutput,
    },
    experiment::{BenchmarkIn, ExperimentError, Procedure, ProcedureId, RunContext, Suite, repeat},
    tools::yosys::{
        Yosys, YosysJob, YosysRunner,
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
pub use output::{McmMeasurements, McmOutput, PaperStats};
pub use paper::{PaperReference, PaperSession, PaperTool, PaperToolId};

pub const VERSION: u32 = 2;
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
        let profile = runner.prepare(
            &YosysJob {
                design: design.clone(),
                assets: Vec::new(),
                commands: vec![mcm.clone(), "stat".into()],
            },
            context,
        )?;
        let baseline = runner.prepare(&SynthFlow::job(&design, &input.synthesis, &[])?, context)?;
        let optimized =
            runner.prepare(&SynthFlow::job(&design, &input.synthesis, &[mcm])?, context)?;
        let mcm = repeat(input.repetitions, |index| {
            let output = runner
                .measure(&profile, &format!("mcm-{index}"), context)
                .map_err(mcm_error)?;
            Ok(YosysBenchmarkOutput::new(
                output.elapsed,
                McmMeasurements::parse(&output, "unoptimized")?,
            ))
        })?;
        let paper = tools.paper.reference(input, context)?;
        let mut synth = |job, phase: &str| -> Result<Vec<SynthOutput>> {
            repeat(input.repetitions, |index| {
                let output = runner
                    .measure(job, &format!("{phase}-{index}"), context)
                    .map_err(mcm_error)?;
                Ok(SynthOutput::new(
                    output.elapsed,
                    SynthMeasurements::parse(&output, Some("unoptimized"))?,
                ))
            })
        };
        let baseline_samples = synth(&baseline, "baseline")?;
        let optimized_samples = synth(&optimized, "optimized")?;
        for (phase, job) in [
            ("mcm", &profile),
            ("baseline", &baseline),
            ("optimized", &optimized),
        ] {
            runner.diagnose_repeated(job, phase, input.repetitions, context)?;
        }
        Ok(McmOutput {
            mcm,
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
