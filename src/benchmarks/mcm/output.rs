use super::PaperReference;
use crate::{
    benchmarks::synth::{SynthMeasurements, SynthOutput, YosysBenchmarkOutput},
    experiment::ExperimentError,
    tools::yosys::YosysRun,
};
use serde::{Deserialize, Serialize};

mod mcm;
mod paper;
pub use mcm::McmMeasurements;
pub use paper::PaperStats;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmOutput {
    pub paper: PaperReference,
    /// Full synthesis and mapping without MCM.
    pub baseline: SynthOutput,
    /// MCM statistics and mapped synthesis from the same Yosys invocation.
    pub optimized: OptimizedOutput,
}

pub type OptimizedOutput = YosysBenchmarkOutput<OptimizedMeasurements>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedMeasurements {
    pub mcm: McmMeasurements,
    pub synthesis: SynthMeasurements,
}

impl OptimizedMeasurements {
    pub fn parse(run: &YosysRun) -> Result<Self, ExperimentError> {
        Ok(Self {
            mcm: McmMeasurements::parse(
                run.log("mcm")?,
                &run.statistics("mcm-stat")?,
                run.pass_time("mcm")?,
            )?,
            synthesis: SynthMeasurements::parse(run)?,
        })
    }
}

crate::benchmark_metrics! {
    OptimizedMeasurements {
        group mcm(McmMeasurements);
        group synthesis(SynthMeasurements);
    }
}

crate::benchmark_metrics! {
    McmOutput {
        group paper(PaperReference);
        group baseline(SynthOutput);
        group optimized(OptimizedOutput);
        computed area_savings(Float, "Area saved against baseline", "%", false, McmOutput::area_savings);
    }
}

impl McmOutput {
    fn area_savings(&self) -> Option<f64> {
        let baseline = (*self.baseline.measurements().area())?;
        let optimized = (*self.optimized.measurements().synthesis.area())?;
        (baseline > 0.0).then_some(100.0 * (1.0 - optimized / baseline))
    }
}
