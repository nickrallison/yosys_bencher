use std::{collections::HashMap, hash::Hash, marker::PhantomData, path::PathBuf, time::Duration};

use contracts::contract_trait;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    experiment::{Benchmark, BenchmarkIn, BenchmarkOut, ExperimentError, RunContext},
    metrics::{MetricDescriptor, MetricType, MetricValue},
    tools::yosys::Yosys,
};

pub mod design;

pub trait YosysProcedure {
    type Config: Clone + Eq + Hash + Serialize + DeserializeOwned;
    type Measurements: BenchmarkOut;

    // ...
}

pub struct YosysBenchmark<P: YosysProcedure> {
    procedure: PhantomData<P>,
}

impl<P: YosysProcedure> Benchmark for YosysBenchmark<P> {
    type Input = YosysBenchmarkInput<P::Config>;
    type Output = YosysBenchmarkOutput<P::Measurements>;

    fn run(
        &self,
        _input: &Self::Input,
        _context: &mut RunContext,
    ) -> Result<Self::Output, ExperimentError> {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
pub struct YosysBenchmarkInput<C = ()> {
    #[get = "pub"]
    path: Yosys,
    #[get = "pub"]
    design: design::Design,
    #[get = "pub"]
    config: C,
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

pub type CellLibrary = PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
pub struct SynthConfig {
    #[get = "pub"]
    cell_lib: CellLibrary,
    #[get = "pub"]
    custom_commands: Vec<String>,
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
