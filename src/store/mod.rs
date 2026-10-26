#![allow(deprecated)]

use std::{collections::HashMap, fmt::Display};

use crate::{
    experiment::{AttemptRecord, BenchmarkIn, BenchmarkOut, ProcedureId, Version},
    src_manager::git::history::CommitId,
};

pub mod error;

pub use error::StoreError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Getters)]
#[getset(get = "pub")]
#[deprecated(
    note = "Temporary store interface sketch; identifiers may change with the real store."
)]
pub struct ExperimentId {
    name: String,
}

impl ExperimentId {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Display for ExperimentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[deprecated(note = "Temporary store interface sketch; the real storage API is not implemented.")]
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

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
#[deprecated(
    note = "Temporary result model; its serialized layout is not a stable storage format."
)]
pub struct ExperimentResults<BMI, BMO> {
    suites: HashMap<CommitId, SuiteResults<BMI, BMO>>,
}

impl<BMI, BMO> ExperimentResults<BMI, BMO> {
    pub fn new(suites: HashMap<CommitId, SuiteResults<BMI, BMO>>) -> Self {
        Self { suites }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
#[deprecated(
    note = "Temporary result model; its serialized layout is not a stable storage format."
)]
pub struct SuiteResults<BMI, BMO> {
    procedure: ProcedureId,
    benchmarks: Vec<BenchmarkResults<BMI, BMO>>,
}

impl<BMI, BMO> SuiteResults<BMI, BMO> {
    pub fn version(&self) -> &Version {
        self.procedure.version()
    }

    pub fn new(procedure: ProcedureId, benchmarks: Vec<BenchmarkResults<BMI, BMO>>) -> Self {
        Self {
            procedure,
            benchmarks,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
#[deprecated(
    note = "Temporary result model; its serialized layout is not a stable storage format."
)]
pub struct BenchmarkResults<BMI, BMO> {
    input: BMI,
    attempts: Vec<AttemptRecord<BMO>>,
}

impl<BMI, BMO> BenchmarkResults<BMI, BMO> {
    pub fn new(input: BMI) -> Self {
        Self {
            input,
            attempts: Vec::new(),
        }
    }

    pub fn record(&mut self, attempt: AttemptRecord<BMO>) {
        self.attempts.push(attempt);
    }
}
