use crate::{
    experiment::{
        BenchmarkIn, BenchmarkOut, ExperimentError, Procedure, ProcedureId, ProcessOutput,
        RunContext,
    },
    metrics::{MetricDescriptor, MetricType, MetricValue},
    tools::yosys::{Yosys, YosysJob, YosysRunner, design},
};
use contracts::contract_trait;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{hash::Hash, time::Duration};

type Result<T> = std::result::Result<T, ExperimentError>;

pub trait YosysProcedure {
    type Config: BenchmarkIn;
    type Measurements: BenchmarkOut;

    fn id(&self) -> ProcedureId;
    fn job(&self, input: &YosysBenchmarkInput<Self::Config>) -> Result<YosysJob>;
    fn parse(
        &self,
        input: &YosysBenchmarkInput<Self::Config>,
        output: &ProcessOutput,
    ) -> Result<Self::Measurements>;
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
        yosys: &Yosys,
        context: &mut RunContext,
    ) -> Result<Self::Output> {
        let runner = YosysRunner::new(yosys)?;
        let job = runner.prepare(&self.procedure.job(input)?, context)?;
        let output = runner.measure(&job, "yosys", context)?;
        let measurements = self.procedure.parse(input, &output)?;
        runner.diagnose(&job, "yosys", context)?;
        Ok(YosysBenchmarkOutput::new(output.elapsed, measurements))
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
        metrics.push(MetricDescriptor::group("measurements", M::list_metrics()));
        metrics
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "runtime" => Some(MetricValue::Duration(self.runtime)),
            "measurements" => Some(self.measurements.metrics()),
            _ => None,
        }
    }
}
