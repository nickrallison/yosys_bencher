use contracts::contract_trait;
use serde::{Deserialize, Serialize};

use super::{SynthConfig, SynthFlow, SynthMeasurements, SynthOutput};
use crate::{
    benchmarks::yosys::{YosysBenchmarkOutput, measure},
    experiment::{BenchmarkOut, ExperimentError, RunContext},
    metrics::{MetricDescriptor, MetricType, MetricValue},
    tools::yosys::{Yosys, YosysJob, YosysRun, YosysRunner, design::Design},
};

type Result<T> = std::result::Result<T, ExperimentError>;

pub type OptimizedOutput<M> = YosysBenchmarkOutput<OptimizedMeasurements<M>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedMeasurements<M> {
    pub optimization: M,
    pub synthesis: SynthMeasurements,
}

#[contract_trait]
impl<M: BenchmarkOut> BenchmarkOut for OptimizedMeasurements<M> {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::group("optimization", M::list_metrics()),
            MetricDescriptor::group("synthesis", SynthMeasurements::list_metrics()),
        ]
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "optimization" => Some(self.optimization.metrics()),
            "synthesis" => Some(self.synthesis.metrics()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonOutput<M, R = ()> {
    pub reference: R,
    pub baseline: SynthOutput,
    pub optimized: OptimizedOutput<M>,
}

impl<M, R> ComparisonOutput<M, R> {
    pub fn area_savings(&self) -> Option<f64> {
        let baseline = (*self.baseline.measurements().area())?;
        let optimized = (*self.optimized.measurements().synthesis.area())?;
        (baseline > 0.0).then_some(100.0 * (1.0 - optimized / baseline))
    }
}

#[contract_trait]
impl<M: BenchmarkOut, R: BenchmarkOut> BenchmarkOut for ComparisonOutput<M, R> {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![
            MetricDescriptor::group("reference", R::list_metrics()),
            MetricDescriptor::group("baseline", SynthOutput::list_metrics()),
            MetricDescriptor::group("optimized", OptimizedOutput::<M>::list_metrics()),
            MetricDescriptor::new(
                "area_savings",
                "Area saved against baseline",
                MetricType::Float,
                "%",
                false,
            ),
        ]
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        match name {
            "reference" => Some(self.reference.metrics()),
            "baseline" => Some(self.baseline.metrics()),
            "optimized" => Some(self.optimized.metrics()),
            "area_savings" => self.area_savings().map(MetricValue::Float),
            _ => None,
        }
    }
}

pub fn compare<M, R>(
    yosys: &Yosys,
    design: &Design,
    config: &SynthConfig,
    context: &mut RunContext,
    optimize: impl FnOnce(&mut YosysJob) -> Result<()>,
    parse: impl FnOnce(&YosysRun) -> Result<M>,
    reference: impl FnOnce(&mut RunContext) -> Result<R>,
) -> Result<ComparisonOutput<M, R>> {
    let runner = YosysRunner::new(yosys)?;
    let baseline = runner.prepare(&SynthFlow::job(design, config)?, context)?;
    let mut job = YosysJob::new(design.clone())?;
    job.timings = true;
    optimize(&mut job)?;
    SynthFlow::append(&mut job, config)?;
    let optimized = runner.prepare(&job, context)?;
    let optimized_output = measure(&runner, &optimized, "optimized", context, |run| {
        Ok(OptimizedMeasurements {
            optimization: parse(run)?,
            synthesis: SynthMeasurements::parse(run)?,
        })
    })?;
    let reference = reference(context)?;
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
    Ok(ComparisonOutput {
        reference,
        baseline: baseline_output,
        optimized: optimized_output,
    })
}
