use crate::{
    experiment::Filter,
    metrics::Reduction,
    store::{ResultSelection, RunSelection},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Operation {
    #[default]
    History,
    Changes {
        comparison: Comparison,
        threshold: Threshold,
        ranking: Ranking,
    },
    Scaling {
        x: String,
        reduce_remaining: bool,
    },
    Variation {
        population: Population,
    },
    Coverage,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Population {
    WithinRun,
    BetweenRuns,
    AcrossInputs,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CoveragePolicy {
    Available,
    Common,
    #[default]
    Strict,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RunTreatment {
    Separate,
    Summaries { reduction: Reduction },
    Pool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "field", rename_all = "snake_case")]
pub enum Dimension {
    Input,
    Parameter(String),
    Run,
    Environment,
    Option(String),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Absolute,
    Ratio,
    Percent,
}
impl ChangeKind {
    pub fn apply(self, current: f64, baseline: f64) -> Option<f64> {
        let v = match self {
            Self::Absolute => current - baseline,
            Self::Ratio if baseline > 0.0 => current / baseline,
            Self::Percent if baseline > 0.0 => 100.0 * (current - baseline) / baseline,
            _ => return None,
        };
        v.is_finite().then_some(v)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Normalization {
    pub revision: String,
    pub kind: ChangeKind,
    pub runs: Option<RunSelection>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Comparison {
    Pair { baseline: String, current: String },
    AdjacentSelected,
    PreviousMeasured,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Threshold {
    pub value: f64,
    pub kind: ChangeKind,
}
impl Default for Threshold {
    fn default() -> Self {
        Self {
            value: 0.0,
            kind: ChangeKind::Percent,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ranking {
    Absolute,
    Percent,
    Contribution,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeClass {
    Improved,
    WithinThreshold,
    Worsened,
    Unclassified,
    Unavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cohort {
    pub baseline: String,
    pub current: String,
    pub threshold: Threshold,
    pub class: ChangeClass,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterStage {
    Attempt,
    Input,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculationFilter {
    pub stage: FilterStage,
    pub filter: Filter,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Paging {
    pub offset: usize,
    pub limit: usize,
}
impl Default for Paging {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 100,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisPlan {
    pub selection: ResultSelection,
    #[serde(default)]
    pub operation: Operation,
    pub metric: Option<String>,
    #[serde(default)]
    pub repetitions: Reduction,
    #[serde(default)]
    pub runs: Option<RunTreatment>,
    #[serde(default)]
    pub inputs: Reduction,
    #[serde(default)]
    pub series: Vec<Dimension>,
    #[serde(default)]
    pub coverage: CoveragePolicy,
    #[serde(default)]
    pub normalization: Option<Normalization>,
    #[serde(default)]
    pub cohort: Option<Cohort>,
    #[serde(default)]
    pub filters: Vec<CalculationFilter>,
    #[serde(default)]
    pub display_filters: Vec<Filter>,
    #[serde(default)]
    pub paging: Paging,
}
impl AnalysisPlan {
    pub fn history(selection: ResultSelection, metric: impl Into<String>) -> Self {
        Self {
            selection,
            operation: Operation::History,
            metric: Some(metric.into()),
            repetitions: Reduction::Mean,
            runs: None,
            inputs: Reduction::Mean,
            series: vec![],
            coverage: CoveragePolicy::Strict,
            normalization: None,
            cohort: None,
            filters: vec![],
            display_filters: vec![],
            paging: Paging::default(),
        }
    }
}
