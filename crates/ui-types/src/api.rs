use crate::{experiment::InputField, metrics::MetricSpec, store::*};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperimentDescription {
    pub default_metric: Option<String>,
    pub experiment: ExperimentSummary,
    pub versions: Vec<DefinitionSummary>,
    pub metrics: Vec<MetricSpec>,
    pub fields: Vec<InputField>,
    pub run_fields: Vec<InputField>,
    pub operations: Vec<String>,
    pub unsupported: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Validation,
    NotFound,
    Unsupported,
    Expired,
    Storage,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiError {
    pub kind: ErrorKind,
    pub field: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputRequest {
    pub selection: ResultSelection,
    pub search: String,
    pub paging: crate::analysis::Paging,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InspectionRequest {
    pub snapshot: crate::analysis::Snapshot,
    pub row: String,
    pub paging: crate::analysis::Paging,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RevisionDescription {
    pub id: String,
    pub committed_at: std::time::SystemTime,
    pub author: String,
    pub message: String,
}
