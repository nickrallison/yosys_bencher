use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Lower,
    Higher,
    Unspecified,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Finite,
    Nonnegative,
    Positive,
}
impl Domain {
    pub fn accepts(self, value: f64) -> bool {
        value.is_finite()
            && match self {
                Self::Finite => true,
                Self::Nonnegative => value >= 0.0,
                Self::Positive => value > 0.0,
            }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Reduction {
    #[default]
    Mean,
    Median,
    Min,
    Max,
    Sum,
    Count,
    StdDev,
    P90,
    P95,
    GeometricMean,
}
impl Reduction {
    /// Quantiles use linear interpolation at (n - 1) * p. SD is sample SD.
    pub fn apply(self, values: &[f64]) -> Option<f64> {
        if values.is_empty() || values.iter().any(|x| !x.is_finite()) {
            return None;
        }
        let scale = values.iter().map(|v| v.abs()).fold(0.0, f64::max);
        let mean = || {
            if scale == 0.0 {
                0.0
            } else {
                values.iter().map(|v| v / scale).sum::<f64>() / values.len() as f64 * scale
            }
        };
        let value = match self {
            Self::Mean => mean(),
            Self::Sum => values.iter().sum(),
            Self::Count => values.len() as f64,
            Self::Min => values.iter().copied().reduce(f64::min)?,
            Self::Max => values.iter().copied().reduce(f64::max)?,
            Self::GeometricMean => {
                if values.iter().any(|v| *v <= 0.0) {
                    return None;
                }
                (values.iter().map(|v| v.ln()).sum::<f64>() / values.len() as f64).exp()
            }
            Self::StdDev => {
                if values.len() < 2 {
                    return None;
                }
                if scale == 0.0 {
                    0.0
                } else {
                    let m = mean() / scale;
                    (values.iter().map(|v| (v / scale - m).powi(2)).sum::<f64>()
                        / (values.len() - 1) as f64)
                        .sqrt()
                        * scale
                }
            }
            Self::Median | Self::P90 | Self::P95 => {
                let mut sorted = values.to_vec();
                sorted.sort_by(f64::total_cmp);
                let p = match self {
                    Self::Median => 0.5,
                    Self::P90 => 0.9,
                    _ => 0.95,
                };
                let pos = (sorted.len() - 1) as f64 * p;
                let f = pos.fract();
                sorted[pos.floor() as usize] * (1.0 - f) + sorted[pos.ceil() as usize] * f
            }
        };
        value.is_finite().then_some(value)
    }
    pub fn estimates() -> Vec<Self> {
        vec![
            Self::Mean,
            Self::Median,
            Self::Min,
            Self::Max,
            Self::P90,
            Self::P95,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaKind {
    Ratio,
    PercentSavings,
    Difference,
}
impl FormulaKind {
    pub fn apply(self, a: f64, b: f64) -> Option<f64> {
        let value = match self {
            Self::Difference => a - b,
            Self::Ratio if b > 0.0 => a / b,
            Self::PercentSavings if b > 0.0 => 100.0 * (1.0 - a / b),
            _ => return None,
        };
        value.is_finite().then_some(value)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaStage {
    Repetition,
    Input,
    Suite,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MetricSource {
    Path,
    Formula {
        numerator: String,
        denominator: String,
        formula: FormulaKind,
        stage: FormulaStage,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricSpec {
    pub path: String,
    pub label: String,
    pub description: String,
    pub kind: MetricType,
    pub unit: String,
    pub direction: Direction,
    pub domain: Domain,
    pub repetitions: Vec<Reduction>,
    pub runs: Vec<Reduction>,
    pub inputs: Vec<Reduction>,
    pub default_repetitions: Reduction,
    pub default_runs: Reduction,
    pub default_inputs: Reduction,
    pub reference: Option<f64>,
    pub source: MetricSource,
}
impl MetricSpec {
    pub fn prefixed(mut self, prefix: &str) -> Self {
        self.path = format!("{prefix}.{}", self.path);
        self.label = format!("{prefix}.{}", self.label);
        if let MetricSource::Formula {
            numerator,
            denominator,
            ..
        } = &mut self.source
        {
            *numerator = format!("{prefix}.{numerator}");
            *denominator = format!("{prefix}.{denominator}");
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricType {
    Int,
    UInt,
    Float,
    Duration,
}
