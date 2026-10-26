use super::PaperReference;
use crate::benchmarks::synth::comparison;

mod mcm;
mod paper;
pub use mcm::McmMeasurements;
pub use paper::PaperStats;

pub type McmOutput = comparison::ComparisonOutput<McmMeasurements, PaperReference>;
pub type OptimizedMeasurements = comparison::OptimizedMeasurements<McmMeasurements>;
pub type OptimizedOutput = comparison::OptimizedOutput<McmMeasurements>;
