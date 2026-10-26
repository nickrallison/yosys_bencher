use std::ops::RangeInclusive;

use rand::{Rng, SeedableRng, rngs::SmallRng};
use serde::{Deserialize, Serialize};

use crate::experiment::ExperimentError as Error;
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CoefficientSets {
    Explicit(Vec<Vec<i64>>),
    Generated(CoefficientGenerator),
}

impl CoefficientSets {
    pub fn len(&self) -> usize {
        match self {
            Self::Explicit(values) => values.len(),
            Self::Generated(generator) => generator.count,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn generate(&self) -> Result<Vec<Vec<i64>>> {
        match self {
            Self::Explicit(values) => Ok(values.clone()),
            Self::Generated(generator) => generator.generate(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoefficientGenerator {
    pub seed: u64,
    pub count: usize,
    pub length_range: RangeInclusive<u32>,
    pub value_range: RangeInclusive<i64>,
    pub a: f64,
    pub scale: f64,
}

impl Default for CoefficientGenerator {
    fn default() -> Self {
        Self {
            seed: 42,
            count: 3,
            length_range: 1..=6,
            value_range: -2_147_483_648..=2_147_483_647,
            a: 1.3,
            scale: 1024.0,
        }
    }
}

impl CoefficientGenerator {
    pub fn generate(&self) -> Result<Vec<Vec<i64>>> {
        if self.length_range.is_empty()
            || *self.length_range.start() == 0
            || self.value_range.is_empty()
            || !self.a.is_finite()
            || self.a <= 1.0
            || !self.scale.is_finite()
            || self.scale <= 0.0
        {
            return Err(Error::Config(
                "invalid coefficient generator settings".into(),
            ));
        }
        let mut random = SmallRng::seed_from_u64(self.seed);
        let mut workloads = Vec::with_capacity(self.count);
        for _ in 0..self.count {
            let length = random.random_range(self.length_range.clone());
            let mut coefficients = Vec::with_capacity(length as usize);
            for _ in 0..length {
                let mut accepted = None;
                for _ in 0..1_000_000 {
                    let pareto = (1.0 - random.random::<f64>()).powf(-1.0 / (self.a - 1.0));
                    let magnitude = (self.scale * (pareto - 1.0)).trunc();
                    let sign = if random.random_bool(0.5) { -1 } else { 1 };
                    if magnitude.is_finite() && magnitude < 9_223_372_036_854_775_808.0 {
                        let value = (magnitude as i64) * sign;
                        if self.value_range.contains(&value) {
                            accepted = Some(value);
                            break;
                        }
                    }
                }
                coefficients.push(accepted.ok_or_else(|| {
                    Error::Config(
                        "coefficient generator exhausted rejection limit; widen the value range"
                            .into(),
                    )
                })?);
            }
            workloads.push(coefficients);
        }
        Ok(workloads)
    }
}
