pub use crate::benchmarks::yosys::{
    YosysBenchmark, YosysBenchmarkInput, YosysBenchmarkOutput, YosysProcedure,
};
pub use crate::tools::yosys::design;
use crate::{
    experiment::{BenchmarkIn, BenchmarkOut, ExperimentError, ProcedureId, ProcessOutput, Suite},
    metrics::{MetricDescriptor, MetricType, MetricValue},
    tools::yosys::{Statistics, Yosys, YosysJob, argument, design::SourceFile, hierarchy_commands},
};
use contracts::contract_trait;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

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

/// Library contents are workload identity; a path alone cannot detect edits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
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
        Ok(Self::new(std::fs::read_to_string(path)?))
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

#[contract_trait]
impl BenchmarkOut for SynthMeasurements {
    fn list_metrics() -> Vec<MetricDescriptor> {
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

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "area" => self.area.map(MetricValue::Float),
            "cell_count" => self
                .cell_count
                .values()
                .try_fold(0_u64, |total, count| total.checked_add(*count))
                .map(MetricValue::UInt),
            _ => None,
        }
    }
}

pub type TimingInput = YosysBenchmarkInput<TimingConfig>;
pub type SynthInput = YosysBenchmarkInput<SynthConfig>;
pub type TimingOutput = YosysBenchmarkOutput<()>;
pub type SynthOutput = YosysBenchmarkOutput<SynthMeasurements>;

impl BenchmarkIn for TimingConfig {}
impl BenchmarkIn for SynthConfig {}

pub struct TimingProcedure;

impl YosysProcedure for TimingProcedure {
    type Config = TimingConfig;
    type Measurements = ();

    fn id(&self) -> ProcedureId {
        ProcedureId::new("yosys.timing", 1)
    }

    fn job(&self, input: &TimingInput) -> Result<YosysJob> {
        let mut commands = hierarchy_commands(input.design())?;
        commands.extend(input.config().custom_commands().iter().cloned());
        Ok(YosysJob {
            design: input.design().clone(),
            assets: Vec::new(),
            commands,
            ..YosysJob::default()
        })
    }

    fn parse(&self, _input: &TimingInput, _output: &ProcessOutput) -> Result<()> {
        Ok(())
    }
}

pub struct SynthFlow;

impl SynthFlow {
    pub fn job(
        design: &design::Design,
        config: &SynthConfig,
        before_synth: &[String],
    ) -> Result<YosysJob> {
        let mut job = YosysJob {
            design: design.clone(),
            assets: vec![SourceFile::new("cells.lib", config.cell_lib.liberty())],
            commands: hierarchy_commands(design)?,
            ..YosysJob::default()
        };
        if !before_synth.is_empty() {
            job.tee("before-synth", before_synth)?;
        }
        job.commands.extend(config.custom_commands.iter().cloned());
        let top = design
            .top
            .as_deref()
            .map(argument)
            .transpose()?
            .map(|top| format!(" -top {top}"))
            .unwrap_or_default();
        job.tee("synth", &[format!("synth{top} -noabc")])?;
        job.tee("abc", &["abc -liberty cells.lib".into()])?;
        job.tee("stat", &["stat -liberty cells.lib".into()])?;
        Ok(job)
    }
}

impl SynthMeasurements {
    pub fn parse(output: &ProcessOutput, top: Option<&str>) -> Result<Self> {
        let stats = Statistics::parse(output.stdout()?, top)?;
        Ok(Self {
            area: stats.area,
            cell_count: stats.cell_count,
        })
    }
}

pub struct SynthProcedure;

impl YosysProcedure for SynthProcedure {
    type Config = SynthConfig;
    type Measurements = SynthMeasurements;

    fn id(&self) -> ProcedureId {
        ProcedureId::new("yosys.synth", 1)
    }
    fn job(&self, input: &SynthInput) -> Result<YosysJob> {
        SynthFlow::job(input.design(), input.config(), &[])
    }
    fn parse(&self, input: &SynthInput, output: &ProcessOutput) -> Result<SynthMeasurements> {
        SynthMeasurements::parse(output, input.design().top.as_deref())
    }
}

/// One workload per design/library pair. Inputs are regenerated each invocation.
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
    fn inputs(&self) -> Result<Vec<SynthInput>> {
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
