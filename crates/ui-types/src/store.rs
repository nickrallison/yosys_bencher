use crate::experiment::{AttemptRecord, Filter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

macro_rules! ids {
    ($($name:ident),*) => {$ (
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub i64);
        impl std::fmt::Display for $name { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.fmt(f) } }
    )*};
}
ids!(
    ExperimentId,
    DefinitionId,
    InputId,
    RunId,
    WorkItemId,
    AttemptId,
    ArtifactId
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub procedure: String,
    pub input_schema: u32,
    pub output_schema: u32,
    pub measurement: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentSpec {
    pub key: String,
    pub name: String,
    pub description: String,
    pub repository: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentSummary {
    pub id: ExperimentId,
    pub spec: ExperimentSpec,
    pub contract: Contract,
    pub created_at: i64,
    pub last_measured_at: Option<i64>,
    pub successful_attempts: usize,
    pub measured_revisions: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionSummary {
    pub id: DefinitionId,
    pub experiment: ExperimentId,
    pub version: Version,
    pub input_count: usize,
    pub successful_attempts: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub machine: String,
    pub os: String,
    pub arch: String,
    pub attributes: BTreeMap<String, String>,
}
impl Default for Environment {
    fn default() -> Self {
        Self {
            machine: std::fs::read_to_string("/etc/hostname")
                .unwrap_or_else(|_| "local".into())
                .trim()
                .into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            attributes: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    #[default]
    Measurement,
    Diagnostic,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Planned,
    Running,
    Complete,
    BuildFailed,
    Interrupted,
}
impl State {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Complete | Self::BuildFailed)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: RunId,
    pub experiment: ExperimentId,
    pub definition: DefinitionId,
    pub version: Version,
    pub partition: String,
    pub history: String,
    pub environment: Environment,
    pub options: Value,
    pub inputs: Vec<InputId>,
    pub revisions: Vec<String>,
    pub purpose: Purpose,
    pub state: State,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub repetitions: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredRun {
    pub run: RunId,
    pub experiment: ExperimentId,
    pub definition: DefinitionSummary,
    pub inputs: Vec<InputId>,
    pub work_items: BTreeMap<String, BTreeMap<InputId, WorkItemId>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VersionSources {
    Exact,
    #[default]
    CompatibleSupersets,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum InputSelection {
    #[default]
    All,
    Ids {
        ids: Vec<InputId>,
    },
    Matching {
        filters: Vec<Filter>,
        #[serde(default)]
        search: String,
        excluded: Vec<InputId>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RevisionQuery {
    #[default]
    All,
    Range {
        start: Option<String>,
        end: Option<String>,
    },
    Only {
        ids: Vec<String>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RunSelection {
    #[default]
    LatestCompleted,
    Explicit {
        ids: Vec<RunId>,
    },
    All,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PartitionSelection {
    #[default]
    Automatic,
    Only {
        ids: Vec<String>,
    },
    All,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultSelection {
    pub experiment: ExperimentId,
    #[serde(default)]
    pub version: Option<Version>,
    #[serde(default)]
    pub sources: VersionSources,
    #[serde(default)]
    pub revisions: RevisionQuery,
    #[serde(default)]
    pub inputs: InputSelection,
    #[serde(default)]
    pub runs: RunSelection,
    #[serde(default)]
    pub partitions: PartitionSelection,
    #[serde(default)]
    pub run_filters: Vec<Filter>,
}
impl ResultSelection {
    pub fn new(experiment: ExperimentId) -> Self {
        Self {
            experiment,
            version: None,
            sources: VersionSources::default(),
            revisions: RevisionQuery::default(),
            inputs: InputSelection::default(),
            runs: RunSelection::default(),
            partitions: PartitionSelection::default(),
            run_filters: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemSummary {
    pub id: WorkItemId,
    pub run: RunId,
    pub revision: String,
    pub input: InputId,
    pub state: State,
    pub reason: Option<String>,
    pub planned_repetitions: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAttempt<O> {
    pub id: AttemptId,
    pub work_item: WorkItemId,
    pub sample_index: u32,
    pub state: State,
    pub artifact_state: ArtifactState,
    pub started_at: i64,
    pub record: Option<AttemptRecord<O>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactState {
    Pending,
    Discarded,
    Retained,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: ArtifactId,
    pub attempt: AttemptId,
    pub name: String,
    pub available: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionStatus {
    pub revision: String,
    pub state: State,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInspection {
    pub run: RunSummary,
    pub revisions: Vec<RevisionStatus>,
    pub work_items: Vec<WorkItemSummary>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Getters,
)]
#[getset(get = "pub")]
pub struct Version {
    major: u32,
    minor: u32,
}

impl Version {
    pub fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

impl Default for Version {
    fn default() -> Self {
        Self::new(1, 0)
    }
}
