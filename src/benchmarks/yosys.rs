use crate::{
    experiment::{BenchmarkIn, BenchmarkOut, ExperimentError, Procedure, RunContext},
    metrics::{MetricDescriptor, MetricType, MetricValue},
    tools::{
        perf::PerfMeasurements,
        yosys::{PreparedYosysJob, Yosys, YosysJob, YosysRun, YosysRunner, design},
    },
};
use contracts::contract_trait;
use serde::{Deserialize, Serialize};
use std::{hash::Hash, time::Duration};

type Result<T> = std::result::Result<T, ExperimentError>;

pub fn measure<M>(
    runner: &YosysRunner,
    job: &PreparedYosysJob,
    label: &str,
    context: &mut RunContext,
    parse: impl FnOnce(&YosysRun) -> Result<M>,
) -> Result<YosysBenchmarkOutput<M>> {
    let run = runner.measure(job, label, context)?;
    let measurements = parse(&run)?;
    Ok(YosysBenchmarkOutput::new(run.process.elapsed, measurements).with_perf(run.process.perf))
}

pub trait YosysProcedure {
    const STORAGE_KEY: &'static str;
    type Config: BenchmarkIn;
    type Measurements: BenchmarkOut;

    fn name(&self) -> &str;
    fn job(&self, input: &YosysBenchmarkInput<Self::Config>) -> Result<YosysJob>;
    fn parse(
        &self,
        input: &YosysBenchmarkInput<Self::Config>,
        output: &YosysRun,
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

    fn name(&self) -> &str {
        self.procedure.name()
    }

    fn run(
        &self,
        input: &Self::Input,
        yosys: &Yosys,
        context: &mut RunContext,
    ) -> Result<Self::Output> {
        let runner = YosysRunner::new(yosys)?;
        let job = runner.prepare(&self.procedure.job(input)?, context)?;
        let output = measure(&runner, &job, "yosys", context, |run| {
            self.procedure.parse(input, run)
        })?;
        runner.diagnose(&job, "yosys", context);
        Ok(output)
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

impl<C: BenchmarkIn> BenchmarkIn for YosysBenchmarkInput<C> {
    fn fields() -> Vec<crate::experiment::InputField> {
        let mut fields = vec![crate::experiment::InputField::new(
            "design",
            "Design",
            crate::experiment::FieldKind::Text,
        )];
        fields.extend(C::fields());
        fields
    }
    fn parameters(&self) -> std::collections::BTreeMap<String, serde_json::Value> {
        let mut values = self.config.parameters();
        values.insert("design".into(), serde_json::json!(self.label()));
        values
    }
    fn label(&self) -> String {
        self.design.top.clone().unwrap_or_else(|| {
            self.design
                .files
                .iter()
                .map(|f| f.path.to_string_lossy())
                .collect::<Vec<_>>()
                .join(", ")
        })
    }
}
impl<P: YosysProcedure + 'static> crate::store::StoredProcedure for YosysBenchmark<P> {
    const STORAGE_KEY: &'static str = P::STORAGE_KEY;
    const MEASUREMENT_CONTRACT: &'static str = "yosys.v2.wall-and-statistics";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Getters)]
pub struct YosysBenchmarkOutput<M = ()> {
    #[get = "pub"]
    runtime: Duration,
    #[get = "pub"]
    measurements: M,
    #[get = "pub"]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    perf: Option<PerfMeasurements>,
}

impl<M> YosysBenchmarkOutput<M> {
    pub fn new(runtime: Duration, measurements: M) -> Self {
        Self {
            runtime,
            measurements,
            perf: None,
        }
    }

    pub fn with_perf(mut self, perf: Option<PerfMeasurements>) -> Self {
        self.perf = perf;
        self
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
    fn analysis_metrics() -> Vec<crate::metrics::MetricSpec> {
        let mut metrics = crate::metrics::describe(&[Self::runtime_descriptor()]);
        metrics[0].direction = crate::metrics::Direction::Lower;
        metrics.extend(
            M::analysis_metrics()
                .into_iter()
                .map(|m| m.prefixed("measurements")),
        );
        metrics.extend(
            PerfMeasurements::analysis_metrics()
                .into_iter()
                .map(|m| m.prefixed("perf")),
        );
        metrics
    }

    fn list_metrics() -> Vec<MetricDescriptor> {
        let mut metrics = vec![YosysBenchmarkOutput::<M>::runtime_descriptor()];
        metrics.push(MetricDescriptor::group("measurements", M::list_metrics()));
        metrics.push(MetricDescriptor::group(
            "perf",
            PerfMeasurements::list_metrics(),
        ));
        metrics
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "runtime" => Some(MetricValue::Duration(self.runtime)),
            "measurements" => Some(self.measurements.metrics()),
            "perf" => self.perf.as_ref().map(BenchmarkOut::metrics),
            _ => None,
        }
    }
}
