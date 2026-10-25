//! Seeded signed coefficients with heavy-tailed magnitudes.

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

/// Uniform lengths and signs with shifted Pareto magnitudes, filtered to the value range.
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
                "invalid MCM coefficient generator settings".into(),
            ));
        }
        let mut random = SmallRng::seed_from_u64(self.seed);
        let mut workloads = Vec::with_capacity(self.count);
        for _ in 0..self.count {
            let length = random.random_range(self.length_range.clone());
            let mut coefficients = Vec::with_capacity(length as usize);
            for _ in 0..length {
                let mut accepted = None;
                // Bound rejection for impossible or impractically narrow ranges.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_workloads_are_small_repeatable_and_in_range() {
        let generator = CoefficientGenerator::default();
        let workloads = generator.generate().unwrap();
        eprintln!("seed {}: {workloads:?}", generator.seed);
        assert_eq!(workloads.len(), 3);
        assert_eq!(workloads, generator.generate().unwrap());
        for coefficients in workloads {
            assert!(
                generator
                    .length_range
                    .contains(&(coefficients.len() as u32))
            );
            assert!(
                coefficients
                    .iter()
                    .all(|value| generator.value_range.contains(value))
            );
        }
    }

    #[test]
    fn settings_are_repeatable_serializable_and_validated() {
        let generator = CoefficientGenerator {
            count: 10,
            seed: u64::MAX,
            ..Default::default()
        };
        let decoded: CoefficientGenerator =
            serde_json::from_str(&serde_json::to_string(&generator).unwrap()).unwrap();
        assert_eq!(generator.generate().unwrap(), decoded.generate().unwrap());
        let mut invalid = generator;
        invalid.a = 1.0;
        assert!(invalid.generate().is_err());
        invalid.a = f64::NAN;
        assert!(invalid.generate().is_err());
        let explicit = CoefficientSets::Explicit(vec![vec![0, 3, 3, -1]]);
        assert_eq!(explicit.generate().unwrap(), vec![vec![0, 3, 3, -1]]);
    }
}
