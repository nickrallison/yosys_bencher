use std::{f64, marker::PhantomData, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    experiment::{Benchmark, BenchmarkIn, BenchmarkOut, ExperimentError, RunContext},
    metrics::{MetricDescriptor, MetricValue},
    tools::yosys::Yosys,
};

pub mod design;

pub trait YosysProcedure {}

pub struct YosysBenchmark<P: YosysProcedure> {
    procedure: PhantomData<P>,
}

impl<P: YosysProcedure> Benchmark for YosysBenchmark<P> {
    type Input = YosysBenchmarkInput;
    type Output = YosysBenchmarkOutput;

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
    commands: Vec<String>,
}

impl BenchmarkIn for YosysBenchmarkInput {}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Getters)]
pub struct YosysBenchmarkOutput {
    #[get]
    runtime: Duration,
}

impl BenchmarkOut for YosysBenchmarkOutput {
    fn list_metrics() -> &'static [MetricDescriptor] {
        todo!()
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Getters)]
pub struct YosysSynthBenchmarkOutput {
    #[get]
    runtime: Duration,
    #[get]
    area: f64,
}

impl BenchmarkOut for YosysSynthBenchmarkOutput {
    fn list_metrics() -> &'static [MetricDescriptor] {
        todo!()
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        todo!()
    }
}
