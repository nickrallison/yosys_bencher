use crate::{
    benchmarks::{
        synth::{CellLibrary, SynthConfig, SynthFlow, SynthMeasurements},
        yosys::measure,
    },
    experiment::{BenchmarkIn, ExperimentError, Procedure, RunContext, Suite},
    tools::yosys::{Yosys, YosysJob, YosysRunner},
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

impl BenchmarkIn for McmInput {
    fn warnings(&self) -> Vec<String> {
        self.synthesis.warnings()
    }
    fn fields() -> Vec<crate::experiment::InputField> {
        use crate::experiment::{FieldKind as K, InputField as F};
        let mut fields = vec![
            F::new("input_width", "Input width", K::Number),
            F::new("coefficient_count", "Coefficient count", K::Number),
            F::new("coefficients", "Coefficients", K::Text),
            F::new(
                "max_coefficient",
                "Largest coefficient magnitude",
                K::Number,
            ),
            F::new("max_depth", "Maximum depth", K::Number),
            F::new("search_budget", "Search budget", K::Number),
            F::new("paper_seed", "Paper seed", K::Number),
            F::new("paper_bit_width", "Paper bit width", K::Number),
        ];
        fields.extend(SynthConfig::fields().into_iter().map(|mut field| {
            field.path = format!("synthesis.{}", field.path);
            field
        }));
        fields.push(F::new("paper_tool", "Paper tool content hash", K::Text));
        fields
    }
    fn parameters(&self) -> std::collections::BTreeMap<String, serde_json::Value> {
        use serde_json::json;
        let mut values: std::collections::BTreeMap<String, serde_json::Value> = [
            ("input_width", json!(self.input_width)),
            ("coefficient_count", json!(self.coefficients.len())),
            ("coefficients", json!(format!("{:?}", self.coefficients))),
            (
                "max_coefficient",
                json!(self.coefficients.iter().map(|c| c.unsigned_abs()).max()),
            ),
            ("max_depth", json!(self.options.max_depth)),
            ("search_budget", json!(self.options.search_budget)),
            ("paper_seed", json!(self.options.paper_seed)),
            ("paper_bit_width", json!(self.options.paper_bit_width)),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect();
        values.extend(
            self.synthesis
                .parameters()
                .into_iter()
                .map(|(k, v)| (format!("synthesis.{k}"), v)),
        );
        values.insert(
            "paper_tool".into(),
            serde_json::to_value(&self.paper_identity).expect("executable identity is a string"),
        );
        values
    }
    fn label(&self) -> String {
        format!("{}-bit {:?}", self.input_width, self.coefficients)
    }
}

impl crate::store::StoredProcedure for McmProcedure {
    const DEFAULT_METRIC: Option<&'static str> = Some("optimized.runtime");
    const STORAGE_KEY: &'static str = "mcm";
    const MEASUREMENT_CONTRACT: &'static str = "mcm.v2.wall-cpu-area";
}
inventory::submit! { crate::analysis::AdapterRegistration::for_procedure::<McmProcedure>() }

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
        let mut job = YosysJob::new(design)?;
        job.stage_with_statistics(
            "mcm",
            vec![format!(
                "mcm -depth {} -search_budget {}",
                input.options.max_depth, input.options.search_budget,
            )],
        )?;
        SynthFlow::append(&mut job, &input.synthesis)?;
        let optimized = runner.prepare(&job, context)?;
        let optimized_output = measure(&runner, &optimized, "optimized", context, |run| {
            Ok(OptimizedMeasurements {
                optimization: McmMeasurements::parse_run(run)?,
                synthesis: SynthMeasurements::parse(run)?,
            })
        })?;
        let reference = tools.paper.reference(input, context)?;
        let baseline_output = measure(
            &runner,
            &baseline,
            "baseline",
            context,
            SynthMeasurements::parse,
        )?;
        for (label, job) in [("baseline", &baseline), ("optimized", &optimized)] {
            runner.diagnose(job, label, context);
        }
        Ok(McmOutput {
            reference,
            baseline: baseline_output,
            optimized: optimized_output,
        })
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
