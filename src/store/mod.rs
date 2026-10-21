use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime},
};

use crate::{experiment::Version, src_manager::git::history::CommitId};

pub struct Store;

pub struct ExperimentResults<BMI, BMO> {
    pub suites: BTreeMap<CommitId, SuiteResults<BMI, BMO>>,
}

pub struct SuiteResults<BMI, BMO> {
    pub version: Version,
    pub benchmarks: Vec<BenchmarkResults<BMI, BMO>>,
}

pub struct BenchmarkResults<BMI, BMO> {
    input: BMI,

    // it is possible to run multiple times on a single benchmark per commit,
    // infact, this might be what is wanted, might be able to do confidence interval
    // that a new version is statistically likely to be different from the previous version
    outputs: Vec<BenchmarkWrapper<BMO>>,
}

pub struct BenchmarkWrapper<BMO> {
    start_time: SystemTime,
    duration: Duration,
    result: BenchmarkResult<BMO>,
}

pub enum BenchmarkResult<BMO> {
    Success(BMO),
    Failure,
}
