use super::plan::*;
use crate::{
    experiment::{InputDescription, Page},
    store::*,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub generation: i64,
    pub plan: AnalysisPlan,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coverage {
    pub selected_inputs: usize,
    pub eligible_inputs: usize,
    pub included_inputs: usize,
    pub valid_repetitions: usize,
    pub planned_repetitions: usize,
    pub runs: usize,
    pub exclusions: Vec<Exclusion>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exclusion {
    pub input: InputId,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub id: String,
    pub x: Value,
    pub value: Option<f64>,
    pub coverage: Coverage,
    pub contributors: Vec<String>,
    pub details: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Series {
    pub id: String,
    pub label: String,
    pub dimensions: BTreeMap<String, Value>,
    pub points: Vec<Point>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub attempt: AttemptId,
    pub run: RunId,
    pub work_item: WorkItemId,
    pub repetition: u32,
    pub started_at: i64,
    pub value: Option<f64>,
    pub operands: Option<(f64, f64)>,
    pub reason: Option<String>,
    pub identities: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Estimate {
    pub id: String,
    pub revision: String,
    pub input: InputId,
    pub partition: String,
    pub run_group: Option<RunId>,
    pub run_ids: Vec<RunId>,
    pub work_items: Vec<WorkItemId>,
    pub value: Option<f64>,
    pub standard_deviation: Option<f64>,
    pub valid_repetitions: usize,
    pub planned_repetitions: usize,
    pub reasons: Vec<String>,
    pub dependencies: Vec<String>,
    pub newer_unfinished: Vec<WorkItemId>,
    pub operands: Option<(f64, f64)>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRow {
    pub id: String,
    pub series: String,
    pub input: InputId,
    pub partition: String,
    pub run_group: Option<RunId>,
    pub baseline: String,
    pub current: String,
    pub before: Option<f64>,
    pub after: Option<f64>,
    pub absolute: Option<f64>,
    pub percent: Option<f64>,
    pub contribution: Option<f64>,
    pub class: ChangeClass,
    pub reason: Option<String>,
    pub skipped_revisions: Vec<String>,
    pub contributors: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSummary {
    pub series: String,
    pub baseline: String,
    pub current: String,
    pub partition: String,
    pub run_group: Option<RunId>,
    pub selected: usize,
    pub matched: usize,
    pub baseline_only: usize,
    pub current_only: usize,
    pub invalid: usize,
    pub improved: usize,
    pub within_threshold: usize,
    pub worsened: usize,
    pub unclassified: usize,
    pub unavailable: usize,
    pub before: Option<f64>,
    pub after: Option<f64>,
    pub absolute: Option<f64>,
    pub percent: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Distribution {
    pub id: String,
    pub population: Population,
    pub revision: String,
    pub input: Option<InputId>,
    pub partition: String,
    pub run: Option<RunId>,
    pub count: usize,
    pub mean: Option<f64>,
    pub standard_deviation: Option<f64>,
    pub median: Option<f64>,
    pub p90: Option<f64>,
    pub p95: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub contributors: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutcomeCounts {
    pub success: usize,
    pub failure: usize,
    pub unsupported: usize,
    pub unfinished: usize,
    pub missing: usize,
    pub missing_metric: usize,
    pub excluded: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageCell {
    pub id: String,
    pub revision: String,
    pub input: InputId,
    pub partition: String,
    pub run: Option<RunId>,
    pub state: Option<State>,
    pub reason: Option<String>,
    pub planned_repetitions: usize,
    pub outcomes: OutcomeCounts,
    pub work_items: Vec<WorkItemId>,
    pub newer_unfinished: Vec<WorkItemId>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResponse {
    pub id: String,
    pub snapshot: Snapshot,
    pub definition: DefinitionSummary,
    pub selected_runs: Vec<RunId>,
    pub selected_work_items: Vec<WorkItemId>,
    pub unit: Option<String>,
    pub reference: Option<f64>,
    pub inputs: Vec<InputDescription>,
    pub revisions: Vec<String>,
    pub partitions: Vec<String>,
    pub series: Vec<Series>,
    pub changes: Page<ChangeRow>,
    pub change_summaries: Vec<ChangeSummary>,
    pub distributions: Page<Distribution>,
    pub coverage: Vec<CoverageCell>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptInspection {
    pub id: AttemptId,
    pub work_item: WorkItemId,
    pub repetition: u32,
    pub state: State,
    pub started_at: i64,
    pub artifact_state: ArtifactState,
    pub record: Option<crate::experiment::AttemptRecord<Value>>,
    pub artifacts: Vec<Artifact>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inspection {
    pub estimates: Page<Estimate>,
    pub observations: Page<Observation>,
    pub attempts: Page<AttemptInspection>,
    pub runs: Vec<RunSummary>,
}
