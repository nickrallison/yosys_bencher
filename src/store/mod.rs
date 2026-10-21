use std::{
    collections::HashMap,
    fmt::Display,
    time::{Duration, SystemTime},
};

use crate::{
    experiment::{BenchmarkIn, BenchmarkOut, Version},
    src_manager::git::history::CommitId,
};

pub mod error;

pub use error::StoreError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Getters)]
pub struct ExperimentId {
    #[get]
    name: String,
}

impl Display for ExperimentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

pub trait Store {
    fn get_results<BMI, BMO>(
        &self,
        exp_id: ExperimentId,
        commit_id: CommitId,
    ) -> Result<SuiteResults<BMI, BMO>, StoreError>
    where
        BMI: BenchmarkIn,
        BMO: BenchmarkOut;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperimentResults<BMI, BMO> {
    suites: HashMap<CommitId, SuiteResults<BMI, BMO>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SuiteResults<BMI, BMO> {
    version: Version,
    benchmarks: Vec<BenchmarkResults<BMI, BMO>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BenchmarkResults<BMI, BMO> {
    input: BMI,

    // it is possible to run multiple times on a single benchmark per commit,
    // infact, this might be what is wanted, might be able to do confidence interval
    // that a new version is statistically likely to be different from the previous version
    outputs: Vec<BenchmarkWrapper<BMO>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BenchmarkWrapper<BMO> {
    start_time: SystemTime,
    duration: Duration,
    result: BenchmarkResult<BMO>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BenchmarkResult<BMO> {
    Success(BMO),
    Failure(String),
}

#[cfg(test)]
mod tests;
