use std::{collections::HashMap, f64, marker::PhantomData, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    experiment::{Benchmark, BenchmarkIn, BenchmarkOut, ExperimentError, RunContext},
    metrics::{MetricDescriptor, MetricType, MetricValue},
    tools::yosys::Yosys,
};

pub mod design;

pub trait YosysProcedure {
    type Input: BenchmarkIn;
    type Measurements: BenchmarkOut;

    // ...
}

pub struct YosysBenchmark<P: YosysProcedure> {
    procedure: PhantomData<P>,
}

impl<P: YosysProcedure> Benchmark for YosysBenchmark<P> {
    type Input = P::Input;
    type Output = YosysBenchmarkOutput<P::Measurements>;

    fn run(
        &self,
        input: &Self::Input,
        context: &mut RunContext,
    ) -> Result<Self::Output, ExperimentError> {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
pub struct YosysBenchmarkInput {
    #[get]
    path: Yosys,
    #[get]
    design: design::Design,
    #[get]
    commands: Vec<String>,
}

impl BenchmarkIn for YosysBenchmarkInput {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Getters)]
pub struct YosysBenchmarkOutput<M> {
    #[get]
    runtime: Duration,
    #[get]
    measurements: M,
}

impl<M> YosysBenchmarkOutput<M> {
    fn runtime_descriptor() -> MetricDescriptor {
        MetricDescriptor::new(
            "runtime",
            "Time to run the Yosys script",
            MetricType::Duration,
            "s",
            false,
        )
    }
}

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
// ##############
// Synth Specific
// ##############

pub type CellLibrary = PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
pub struct SynthBenchmarkInput {
    #[get]
    cell_lib: CellLibrary,
    #[get]
    custom_commands: Vec<String>,
    #[get]
    yosys_bmi: YosysBenchmarkInput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Getters)]
pub struct SynthMeasurements {
    #[get]
    area: f64,
    #[get]
    cell_count: HashMap<String, u64>,
}

impl BenchmarkOut for SynthMeasurements {
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

pub type TimingOutput = YosysBenchmarkOutput<()>;
pub type SynthOutput = YosysBenchmarkOutput<SynthMeasurements>;
