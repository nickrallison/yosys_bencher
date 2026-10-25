//! MCM and baseline synthesis comparison with a paper reference.

use crate::{
    benchmarks::synth::{CellLibrary, SynthConfig, SynthFlow, SynthMeasurements, SynthOutput},
    experiment::{BenchmarkIn, ExperimentError, Procedure, ProcedureId, RunContext, Suite},
    tools::yosys::{
        Yosys, YosysJob, YosysRunner,
        design::{Design, SourceFile},
        hierarchy_commands,
    },
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

/// Temporary placeholder until suite/procedure versions are derived from the store.
#[deprecated(
    note = "Replace the hardcoded MCM version with version computation from the DB/store and suites."
)]
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

    #[allow(deprecated)] // The placeholder remains in use until the store exists.
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
        let baseline = runner.prepare(&SynthFlow::job(&design, &input.synthesis)?, context)?;
        let mut optimized_job = YosysJob {
            design,
            timings: true,
            ..YosysJob::default()
        };
        optimized_job.stage("hierarchy", hierarchy_commands(&optimized_job.design)?)?;
        optimized_job.stage("mcm", vec![mcm])?;
        optimized_job.stage("mcm-stat", vec!["stat -json".into()])?;
        SynthFlow::append(&mut optimized_job, &input.synthesis)?;
        let optimized = runner.prepare(&optimized_job, context)?;
        let output = runner
            .measure(&optimized, "optimized", context)
            .map_err(mcm_error)?;
        let cpu_time = output
            .pass_timings
            .get("mcm")
            .copied()
            .ok_or_else(|| ExperimentError::Parse("missing MCM pass timing".into()))?;
        let optimized_output = OptimizedOutput::new(
            output.process.elapsed,
            OptimizedMeasurements {
                mcm: McmMeasurements::parse(
                    output.log("mcm")?,
                    &output.statistics("mcm-stat")?,
                    cpu_time,
                )?,
                synthesis: SynthMeasurements::parse(&output)?,
            },
        );
        let paper = tools.paper.reference(input, context)?;
        let output = runner.measure(&baseline, "baseline", context)?;
        let baseline_output =
            SynthOutput::new(output.process.elapsed, SynthMeasurements::parse(&output)?);
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
