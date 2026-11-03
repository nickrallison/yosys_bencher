pub use crate::benchmarks::yosys::{
    YosysBenchmark, YosysBenchmarkInput, YosysBenchmarkOutput, YosysProcedure,
};
pub use crate::tools::yosys::design;
use crate::{
    experiment::{BenchmarkIn, ExperimentError, Suite},
    tools::yosys::{Yosys, YosysJob, YosysRun, argument, design::SourceFile},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::Path,
};

type Result<T> = std::result::Result<T, ExperimentError>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
pub struct TimingConfig {
    #[get = "pub"]
    custom_commands: Vec<String>,
}

impl TimingConfig {
    pub fn new(custom_commands: Vec<String>) -> Self {
        Self { custom_commands }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct CellLibrary {
    liberty: String,
}

impl CellLibrary {
    pub fn new(liberty: impl Into<String>) -> Self {
        Self {
            liberty: liberty.into(),
        }
    }

    pub fn from_path(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        let library = Self::new(std::fs::read_to_string(path)?);
        if let Some(warning) = library.warning() {
            eprintln!("Warning: {}: {warning}", path.display());
        }
        Ok(library)
    }

    /// The declared name identifies a library, including for benchmark input identity.
    pub fn name(&self) -> Option<&str> {
        // Allow copyright headers; the bundled Nangate declaration is on line 37.
        self.liberty.lines().take(128).find_map(|line| {
            let declaration = line.trim_start().strip_prefix("library")?.trim_start();
            let (name, _) = declaration.strip_prefix('(')?.split_once(')')?;
            let name = name.trim().trim_matches('"');
            (!name.is_empty()).then_some(name)
        })
    }

    fn identity(&self) -> (bool, &str) {
        match self.name() {
            Some(name) => (true, name),
            // Keep malformed/unnamed inputs distinct without inventing a shared name.
            None => (false, &self.liberty),
        }
    }
    fn warning(&self) -> Option<&'static str> {
        self.name()
            .is_none()
            .then_some("Cell library has no declared name; using its contents for input identity.")
    }
}

impl PartialEq for CellLibrary {
    fn eq(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
}
impl Eq for CellLibrary {}
impl Hash for CellLibrary {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identity().hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
pub struct SynthConfig {
    #[get = "pub"]
    cell_lib: CellLibrary,
    #[get = "pub"]
    custom_commands: Vec<String>,
}

impl SynthConfig {
    pub fn new(cell_lib: CellLibrary, custom_commands: Vec<String>) -> Self {
        Self {
            cell_lib,
            custom_commands,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Getters)]
pub struct SynthMeasurements {
    #[get = "pub"]
    area: Option<f64>,
    #[get = "pub"]
    cell_count: HashMap<String, u64>,
}

#[contracts::contract_trait]
impl crate::experiment::BenchmarkOut for SynthMeasurements {
    fn observation_sources(&self) -> Option<Vec<(String, String)>> {
        Some(vec![])
    }
    fn get_scalar(&self, path: &str) -> Option<f64> {
        match path {
            "area" => self.area,
            "cell_count" => self
                .cell_count
                .values()
                .try_fold(0_u64, |n, v| n.checked_add(*v))
                .map(|v| v as f64),
            _ => None,
        }
    }
    fn list_metrics() -> Vec<crate::metrics::MetricDescriptor> {
        use crate::metrics::{MetricDescriptor, MetricType};
        vec![
            MetricDescriptor::new(
                "area",
                "Mapped synthesis area",
                MetricType::Float,
                "area",
                false,
            ),
            MetricDescriptor::new(
                "cell_count",
                "Total number of cells",
                MetricType::UInt,
                "cells",
                false,
            ),
        ]
    }
    fn get_metric(&self, name: &str) -> Option<crate::metrics::MetricValue> {
        use crate::metrics::MetricValue;
        match name {
            "area" => self.area.map(MetricValue::Float),
            "cell_count" => self
                .cell_count
                .values()
                .try_fold(0_u64, |sum, count| sum.checked_add(*count))
                .map(MetricValue::UInt),
            _ => None,
        }
    }
    fn analysis_metrics() -> Vec<crate::metrics::MetricSpec> {
        let mut metrics = crate::metrics::describe(&Self::list_metrics());
        for m in &mut metrics {
            m.direction = crate::metrics::Direction::Lower;
            m.domain = crate::metrics::Domain::Nonnegative;
        }
        metrics
    }
}

pub type TimingInput = YosysBenchmarkInput<TimingConfig>;
pub type SynthInput = YosysBenchmarkInput<SynthConfig>;
pub type TimingOutput = YosysBenchmarkOutput<()>;
pub type SynthOutput = YosysBenchmarkOutput<SynthMeasurements>;

impl BenchmarkIn for TimingConfig {
    fn fields() -> Vec<crate::experiment::InputField> {
        vec![crate::experiment::InputField::new(
            "commands",
            "Commands",
            crate::experiment::FieldKind::Text,
        )]
    }
    fn parameters(&self) -> std::collections::BTreeMap<String, serde_json::Value> {
        [(
            "commands".into(),
            serde_json::json!(self.custom_commands.join("; ")),
        )]
        .into()
    }
}
impl BenchmarkIn for SynthConfig {
    fn warnings(&self) -> Vec<String> {
        self.cell_lib
            .warning()
            .map(str::to_owned)
            .into_iter()
            .collect()
    }
    fn fields() -> Vec<crate::experiment::InputField> {
        vec![
            crate::experiment::InputField::new(
                "library",
                "Cell library",
                crate::experiment::FieldKind::Text,
            ),
            crate::experiment::InputField::new(
                "commands",
                "Commands",
                crate::experiment::FieldKind::Text,
            ),
        ]
    }
    fn parameters(&self) -> std::collections::BTreeMap<String, serde_json::Value> {
        [
            (
                "library".into(),
                serde_json::json!(self.cell_lib.name().unwrap_or("Unnamed library")),
            ),
            (
                "commands".into(),
                serde_json::json!(self.custom_commands.join("; ")),
            ),
        ]
        .into()
    }
}

pub struct TimingProcedure;

impl YosysProcedure for TimingProcedure {
    const STORAGE_KEY: &'static str = "yosys.timing";
    type Config = TimingConfig;
    type Measurements = ();

    fn name(&self) -> &str {
        "yosys.timing"
    }

    fn job(&self, input: &TimingInput) -> Result<YosysJob> {
        let mut job = YosysJob::new(input.design().clone())?;
        if !input.config().custom_commands().is_empty() {
            job.stage("commands", input.config().custom_commands().clone())?;
        }
        Ok(job)
    }

    fn parse(&self, _input: &TimingInput, _output: &YosysRun) -> Result<()> {
        Ok(())
    }
}

pub struct SynthFlow;

impl SynthFlow {
    pub fn job(design: &design::Design, config: &SynthConfig) -> Result<YosysJob> {
        let mut job = YosysJob::new(design.clone())?;
        Self::append(&mut job, config)?;
        Ok(job)
    }

    pub fn append(job: &mut YosysJob, config: &SynthConfig) -> Result<()> {
        job.assets
            .push(SourceFile::new("cells.lib", config.cell_lib.liberty()));
        if !config.custom_commands.is_empty() {
            job.stage("custom", config.custom_commands.clone())?;
        }
        let top = job
            .design
            .top
            .as_deref()
            .map(argument)
            .transpose()?
            .map(|top| format!(" -top {top}"))
            .unwrap_or_default();
        job.stage("synth", vec![format!("synth{top} -noabc")])?;
        job.stage("dfflibmap", vec!["dfflibmap -liberty cells.lib".into()])?;
        job.stage("abc", vec!["abc -liberty cells.lib".into()])?;
        job.stage("stat", vec!["stat -json -liberty cells.lib".into()])?;
        Ok(())
    }
}

impl SynthMeasurements {
    pub fn parse(output: &YosysRun) -> Result<Self> {
        Ok(Self::from_statistics(output.statistics("stat")?))
    }

    pub fn from_statistics(stats: crate::tools::yosys::Statistics) -> Self {
        let area = if stats.cell_count.keys().any(|name| name.starts_with('$')) {
            None
        } else if stats.cell_count.is_empty() {
            Some(0.0)
        } else {
            stats.area
        };
        Self {
            area,
            cell_count: stats.cell_count,
        }
    }
}

pub struct SynthProcedure;

impl YosysProcedure for SynthProcedure {
    const STORAGE_KEY: &'static str = "yosys.synth";
    type Config = SynthConfig;
    type Measurements = SynthMeasurements;

    fn name(&self) -> &str {
        "yosys.synth"
    }
    fn job(&self, input: &SynthInput) -> Result<YosysJob> {
        SynthFlow::job(input.design(), input.config())
    }
    fn parse(&self, _input: &SynthInput, output: &YosysRun) -> Result<SynthMeasurements> {
        SynthMeasurements::parse(output)
    }
}

pub struct SynthSuite {
    pub name: String,
    pub designs: Vec<design::Design>,
    pub libraries: Vec<CellLibrary>,
    pub custom_commands: Vec<String>,
    procedure: YosysBenchmark<SynthProcedure>,
}

impl SynthSuite {
    pub fn new(
        name: impl Into<String>,
        designs: Vec<design::Design>,
        libraries: Vec<CellLibrary>,
    ) -> Self {
        Self {
            name: name.into(),
            designs,
            libraries,
            custom_commands: Vec::new(),
            procedure: YosysBenchmark::new(SynthProcedure),
        }
    }
}

impl Suite for SynthSuite {
    type Tool = Yosys;
    type Session = ();
    type Procedure = YosysBenchmark<SynthProcedure>;

    fn name(&self) -> &str {
        &self.name
    }
    fn procedure(&self) -> &Self::Procedure {
        &self.procedure
    }
    fn inputs(&self, _session: &()) -> Result<Vec<SynthInput>> {
        Ok(self
            .designs
            .iter()
            .flat_map(|design| {
                self.libraries.iter().map(|library| {
                    SynthInput::new(
                        design.clone(),
                        SynthConfig::new(library.clone(), self.custom_commands.clone()),
                    )
                })
            })
            .collect())
    }
    fn begin(&self) -> Result<()> {
        Ok(())
    }
    fn tools(&self, tool: &Yosys, _session: &()) -> Result<Yosys> {
        Ok(tool.clone())
    }
}

inventory::submit! { crate::analysis::AdapterRegistration::for_procedure::<YosysBenchmark<TimingProcedure>>() }
inventory::submit! { crate::analysis::AdapterRegistration::for_procedure::<YosysBenchmark<SynthProcedure>>() }

#[cfg(test)]
mod tests;
