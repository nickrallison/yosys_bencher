use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use yosys_bench_ui_types::{analysis::*, api::*, experiment::*, store::*};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    pub description: ExperimentDescription,
    pub revision_order_required: bool,
    pub warnings: Vec<String>,
    pub revisions: Vec<RevisionDescription>,
    pub runs: Vec<RunSummary>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputPage {
    pub page: Page<InputDescription>,
    pub selected: usize,
    pub target: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactData {
    pub name: String,
    pub bytes: Vec<u8>,
}
#[cfg(feature = "server")]
fn backend<T>(
    f: impl FnOnce(yosys_bench::analysis::Backend) -> yosys_bench::store::Result<T>,
) -> Result<T, ApiError> {
    let path = std::env::var_os("YOSYS_BENCH_DATABASE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../output/results.sqlite")
        });
    yosys_bench::analysis::Backend::open(path)
        .and_then(f)
        .map_err(|e| ApiError::from(&e))
}
pub fn result<T>(reply: Result<Result<T, ApiError>, ServerFnError>) -> Result<T, ApiError> {
    reply.map_err(|e| ApiError {
        kind: ErrorKind::Storage,
        field: None,
        message: e.to_string(),
    })?
}
#[server]
pub async fn catalog() -> Result<Result<Vec<ExperimentSummary>, ApiError>, ServerFnError> {
    Ok(backend(|b| b.experiments()))
}
#[server]
pub async fn metadata(id: ExperimentId) -> Result<Result<Metadata, ApiError>, ServerFnError> {
    Ok(backend(|b| {
        let description = b.describe(id)?;
        let runs = b.runs(id)?;
        let (revisions, revision_order_required, warnings) =
            match b.revisions(id, &RevisionQuery::All) {
                Ok(revisions) => (revisions, false, vec![]),
                Err(yosys_bench::store::StoreError::Invalid(reason)) => {
                    let mut seen = std::collections::BTreeSet::new();
                    let ids = runs
                        .iter()
                        .flat_map(|r| r.revisions.iter().cloned())
                        .filter(|id| seen.insert(id.clone()))
                        .collect();
                    (
                        b.revisions(id, &RevisionQuery::Only { ids })?,
                        true,
                        vec![reason],
                    )
                }
                Err(error) => return Err(error),
            };
        Ok(Metadata {
            description,
            revisions,
            runs,
            revision_order_required,
            warnings,
        })
    }))
}
#[server]
pub async fn inputs(request: InputRequest) -> Result<Result<InputPage, ApiError>, ServerFnError> {
    Ok(backend(|b| {
        let selected = b
            .inputs(
                &request.selection,
                "",
                Paging {
                    offset: 0,
                    limit: 0,
                },
            )?
            .total;
        let target = b
            .store
            .target_definition(request.selection.experiment, request.selection.version)?
            .input_count;
        let mut list = request.selection.clone();
        list.inputs = match list.inputs {
            InputSelection::Matching {
                filters, search, ..
            } => InputSelection::Matching {
                filters,
                search,
                excluded: vec![],
            },
            _ => InputSelection::All,
        };
        Ok(InputPage {
            page: b.inputs(&list, &request.search, request.paging)?,
            selected,
            target,
        })
    }))
}
#[server]
pub async fn groups(
    selection: ResultSelection,
    field: String,
) -> Result<Result<Vec<(serde_json::Value, usize)>, ApiError>, ServerFnError> {
    Ok(backend(|b| b.groups(&selection, &field)))
}
#[server]
pub async fn analyze(
    plan: AnalysisPlan,
) -> Result<Result<AnalysisResponse, ApiError>, ServerFnError> {
    Ok(backend(|b| b.analyze(&plan)))
}
#[server]
pub async fn inspect(
    request: InspectionRequest,
) -> Result<Result<Inspection, ApiError>, ServerFnError> {
    Ok(backend(|b| {
        b.inspect(&request.snapshot, &request.row, request.paging)
    }))
}
#[server]
pub async fn inspect_run(id: RunId) -> Result<Result<RunInspection, ApiError>, ServerFnError> {
    Ok(backend(|b| b.inspect_run(id)))
}
#[server]
pub async fn inspect_work(
    id: WorkItemId,
    paging: Paging,
) -> Result<Result<Page<AttemptInspection>, ApiError>, ServerFnError> {
    Ok(backend(|b| b.inspect_work_item(id, paging)))
}
#[server]
pub async fn artifact(
    id: ArtifactId,
    name: String,
) -> Result<Result<ArtifactData, ApiError>, ServerFnError> {
    Ok(backend(|b| {
        Ok(ArtifactData {
            name,
            bytes: b.read_artifact(id)?,
        })
    }))
}
