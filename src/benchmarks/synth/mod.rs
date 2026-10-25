use std::{collections::HashMap, hash::Hash, path::Path, time::Duration};

use contracts::contract_trait;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    experiment::{BenchmarkIn, BenchmarkOut, ExperimentError, Procedure, ProcedureId, RunContext},
    metrics::{MetricDescriptor, MetricType, MetricValue},
    tools::yosys::Yosys,
};

pub mod design;

pub trait YosysProcedure {
    type Config: Clone + Eq + Hash + Serialize + DeserializeOwned;
    type Measurements: BenchmarkOut;

    fn id(&self) -> ProcedureId;

    fn run(
        &self,
        input: &YosysBenchmarkInput<Self::Config>,
        yosys: &Yosys,
        context: &mut RunContext,
    ) -> Result<YosysBenchmarkOutput<Self::Measurements>, ExperimentError>;
}

pub struct YosysBenchmark<P: YosysProcedure> {
    procedure: P,
}

impl<P: YosysProcedure> YosysBenchmark<P> {
    pub fn new(procedure: P) -> Self {
        Self { procedure }
    }
}

impl<P: YosysProcedure> Procedure for YosysBenchmark<P> {
    type Input = YosysBenchmarkInput<P::Config>;
    type Tools = Yosys;
    type Output = YosysBenchmarkOutput<P::Measurements>;

    fn id(&self) -> ProcedureId {
        self.procedure.id()
    }

    fn run(
        &self,
        input: &Self::Input,
        yosys: &Self::Tools,
        context: &mut RunContext,
    ) -> Result<Self::Output, ExperimentError> {
        self.procedure.run(input, yosys, context)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
pub struct YosysBenchmarkInput<C = ()> {
    #[get = "pub"]
    design: design::Design,
    #[get = "pub"]
    config: C,
}

impl<C> YosysBenchmarkInput<C> {
    pub fn new(design: design::Design, config: C) -> Self {
        Self { design, config }
    }
}

impl<C> BenchmarkIn for YosysBenchmarkInput<C> where
    C: Clone + Eq + Hash + Serialize + DeserializeOwned
{
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Getters)]
pub struct YosysBenchmarkOutput<M = ()> {
    #[get = "pub"]
    runtime: Duration,
    #[get = "pub"]
    measurements: M,
}

impl<M> YosysBenchmarkOutput<M> {
    pub fn new(runtime: Duration, measurements: M) -> Self {
        Self {
            runtime,
            measurements,
        }
    }

    fn runtime_descriptor() -> MetricDescriptor {
        MetricDescriptor::new(
            "runtime",
            "Time to run the Yosys script",
            MetricType::Duration,
            "s",
            true,
        )
    }
}

#[contract_trait]
impl<M: BenchmarkOut> BenchmarkOut for YosysBenchmarkOutput<M> {
    fn list_metrics() -> Vec<MetricDescriptor> {
        let mut metrics = vec![YosysBenchmarkOutput::<M>::runtime_descriptor()];
        metrics.extend(M::list_metrics());
        metrics
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "runtime" => Some(MetricValue::Duration(self.runtime)),
            _ => self.measurements.get_metric(name),
        }
    }
}

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
    area: f64,
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
            "area" => Some(MetricValue::Float(self.area)),
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
