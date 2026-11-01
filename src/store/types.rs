use crate::{
    experiment::{BenchmarkIn, InputDescription, Procedure},
    src_manager::git::history::Commit,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
pub use yosys_bench_ui_types::store::*;
pub trait StoredProcedure: Procedure + 'static {
    const DEFAULT_METRIC: Option<&'static str> = None;
    const STORAGE_KEY: &'static str;
    const INPUT_SCHEMA: u32 = 1;
    const OUTPUT_SCHEMA: u32 = 1;
    const MEASUREMENT_CONTRACT: &'static str;
}

pub fn contract_for<P: StoredProcedure>() -> Contract {
    Contract {
        procedure: P::STORAGE_KEY.into(),
        input_schema: P::INPUT_SCHEMA,
        output_schema: P::OUTPUT_SCHEMA,
        measurement: P::MEASUREMENT_CONTRACT.into(),
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPlan {
    pub revisions: Vec<Commit>,
    pub input_indices: Option<Vec<usize>>,
    pub repetitions: u32,
    pub resolved_inputs: Vec<InputId>,
    pub history: String,
    pub environment: Environment,
    pub options: Value,
    pub purpose: Purpose,
}
impl RunPlan {
    pub fn new(revisions: Vec<Commit>, repetitions: u32) -> Self {
        Self {
            revisions,
            input_indices: None,
            repetitions,
            resolved_inputs: vec![],
            history: "first-parent".into(),
            environment: Environment::default(),
            options: serde_json::json!({}),
            purpose: Purpose::Measurement,
        }
    }
}
#[derive(Debug, Clone)]
pub struct ResultFrame<I, O> {
    pub generation: i64,
    pub definition: DefinitionSummary,
    pub revisions: Vec<Commit>,
    pub input_order: Vec<InputId>,
    pub inputs: BTreeMap<InputId, I>,
    pub descriptions: BTreeMap<InputId, InputDescription>,
    pub runs: Vec<RunSummary>,
    pub work_items: Vec<WorkItemSummary>,
    pub attempts: Vec<StoredAttempt<O>>,
    pub newer_unfinished: Vec<WorkItemSummary>,
}
impl<I: BenchmarkIn, O> ResultFrame<I, O> {
    pub fn describe_input(id: InputId, value: &I) -> InputDescription {
        InputDescription {
            id,
            label: value.label(),
            parameters: value.parameters(),
        }
    }
}
