use super::{cache::Cache, engine, plan::*, result::*};
use crate::{
    experiment::{BenchmarkIn, BenchmarkOut, InputDescription, InputField, Page},
    metrics::MetricSpec,
    store::*,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, OnceLock},
};

pub use yosys_bench_ui_types::api::*;
pub trait ExperimentAdapter: Send + Sync {
    fn contract(&self) -> Contract;
    fn metrics(&self) -> Vec<MetricSpec>;
    fn stored_metrics(
        &self,
        store: &SqliteStore,
        experiment: ExperimentId,
    ) -> Result<Vec<MetricSpec>>;
    fn default_metric(&self) -> Option<String>;
    fn fields(&self) -> Vec<InputField>;
    fn inputs(
        &self,
        store: &SqliteStore,
        selection: &ResultSelection,
        search: &str,
        paging: Paging,
    ) -> Result<Page<InputDescription>>;
    fn groups(
        &self,
        store: &SqliteStore,
        selection: &ResultSelection,
        field: &str,
    ) -> Result<Vec<(Value, usize)>>;
    fn analyze(&self, store: &SqliteStore, plan: &AnalysisPlan) -> Result<AnalysisResponse>;
    fn inspect(
        &self,
        store: &SqliteStore,
        snapshot: &Snapshot,
        row: &str,
        paging: Paging,
    ) -> Result<Inspection>;
}
struct TypedAdapter<P: StoredProcedure> {
    catalog: Cache<Vec<MetricSpec>>,
    frames: Cache<AnalysisFrame<P>>,
    evaluations: Cache<engine::Evaluation>,
    input_pages: Cache<Page<InputDescription>>,
}
impl<P: StoredProcedure> TypedAdapter<P> {
    fn catalog(
        &self,
        store: &SqliteStore,
        experiment: ExperimentId,
    ) -> Result<Arc<Vec<MetricSpec>>> {
        store.ensure_projection::<P>(experiment)?;
        self.catalog.get(store, &experiment, || {
            store.projected_metrics::<P>(experiment)
        })
    }
    fn frame(
        &self,
        store: &SqliteStore,
        selection: &ResultSelection,
        paths: &[String],
    ) -> Result<Arc<AnalysisFrame<P>>> {
        self.frames.get(store, &(selection, paths), || {
            store.projected_frame::<P>(selection, paths)
        })
    }
    fn evaluate(
        &self,
        store: &SqliteStore,
        plan: &AnalysisPlan,
    ) -> Result<Arc<engine::Evaluation>> {
        store.ensure_projection::<P>(plan.selection.experiment)?;
        self.evaluations
            .get(store, plan, || self.evaluate_uncached(store, plan))
    }
    fn evaluate_uncached(
        &self,
        store: &SqliteStore,
        plan: &AnalysisPlan,
    ) -> Result<engine::Evaluation> {
        let metrics = self.catalog(store, plan.selection.experiment)?;
        let metric = engine::validate::<P::Input>(plan, &metrics)?;
        let mut paths = match metric {
            None => vec![],
            Some(metric) => match metric.source {
                crate::metrics::MetricSource::Path => vec![metric.path],
                crate::metrics::MetricSource::Formula {
                    numerator,
                    denominator,
                    ..
                } => vec![numerator, denominator],
            },
        };
        paths.sort();
        paths.dedup();
        let mut selection = plan.selection.clone();
        if let Operation::Changes {
            comparison: Comparison::Pair { baseline, current },
            ..
        } = &plan.operation
        {
            selection.revisions = RevisionQuery::Only {
                ids: if baseline == current {
                    vec![baseline.clone()]
                } else {
                    vec![baseline.clone(), current.clone()]
                },
            };
        }
        let frame = self.frame(store, &selection, &paths)?;
        let revisions = frame
            .revisions
            .iter()
            .map(|r| r.id().id().clone())
            .collect::<Vec<_>>();
        let baseline = if let Some(normalization) = &plan.normalization {
            let mut s = selection.clone();
            s.revisions = RevisionQuery::Only {
                ids: vec![normalization.revision.clone()],
            };
            if let Some(runs) = &normalization.runs {
                s.runs = runs.clone();
            }
            Some(self.frame(store, &s, &paths)?)
        } else {
            None
        };
        let cohort = if let Some(cohort) = &plan.cohort {
            let mut s = selection.clone();
            s.revisions = RevisionQuery::Only {
                ids: if cohort.baseline == cohort.current {
                    vec![cohort.baseline.clone()]
                } else {
                    vec![cohort.baseline.clone(), cohort.current.clone()]
                },
            };
            Some(self.frame(store, &s, &paths)?)
        } else {
            None
        };
        let result = engine::evaluate(
            &frame,
            plan,
            &revisions,
            baseline.as_deref(),
            cohort.as_deref(),
            &metrics,
        )?;
        if store.generation()? != frame.generation {
            return Err(StoreError::Expired);
        }
        Ok(result)
    }
}
impl<P: StoredProcedure> ExperimentAdapter for TypedAdapter<P>
where
    P::Input: Send + Sync,
    P::Output: Send + Sync,
{
    fn contract(&self) -> Contract {
        contract_for::<P>()
    }
    fn metrics(&self) -> Vec<MetricSpec> {
        P::Output::analysis_metrics()
    }
    fn stored_metrics(
        &self,
        store: &SqliteStore,
        experiment: ExperimentId,
    ) -> Result<Vec<MetricSpec>> {
        Ok((*self.catalog(store, experiment)?).clone())
    }
    fn default_metric(&self) -> Option<String> {
        P::DEFAULT_METRIC
            .map(str::to_owned)
            .or_else(|| self.metrics().first().map(|m| m.path.clone()))
    }
    fn fields(&self) -> Vec<InputField> {
        P::Input::fields()
    }
    fn inputs(
        &self,
        store: &SqliteStore,
        s: &ResultSelection,
        search: &str,
        p: Paging,
    ) -> Result<Page<InputDescription>> {
        store.ensure_projection::<P>(s.experiment)?;
        Ok((*self.input_pages.get(store, &(s, search, p), || {
            store.inputs::<P>(s, search, p.offset, p.limit)
        })?)
        .clone())
    }
    fn groups(
        &self,
        store: &SqliteStore,
        s: &ResultSelection,
        f: &str,
    ) -> Result<Vec<(Value, usize)>> {
        store.groups::<P>(s, f)
    }
    fn analyze(&self, store: &SqliteStore, plan: &AnalysisPlan) -> Result<AnalysisResponse> {
        self.evaluate(store, plan).map(|r| r.response.clone())
    }
    fn inspect(
        &self,
        store: &SqliteStore,
        snapshot: &Snapshot,
        row: &str,
        paging: Paging,
    ) -> Result<Inspection> {
        if store.generation()? != snapshot.generation {
            return Err(StoreError::Expired);
        }
        let evaluation = self.evaluate(store, &snapshot.plan)?;
        if evaluation.response.snapshot.generation != snapshot.generation {
            return Err(StoreError::Expired);
        }
        let response = &evaluation.response;
        let mut ids = BTreeSet::new();
        let mut work_ids = BTreeSet::new();
        let mut found = false;
        if let Some(point) = response
            .series
            .iter()
            .flat_map(|s| &s.points)
            .find(|p| p.id == row)
        {
            ids.extend(point.contributors.iter().cloned());
            found = true;
        }
        if let Some(change) = response.changes.items.iter().find(|r| r.id == row) {
            ids.extend(change.contributors.iter().cloned());
            found = true;
        }
        if let Some(distribution) = response.distributions.items.iter().find(|r| r.id == row) {
            ids.extend(distribution.contributors.iter().cloned());
            found = true;
        }
        if let Some(cell) = response.coverage.iter().find(|c| c.id == row) {
            work_ids.extend(cell.work_items.iter().copied());
            found = true;
        }
        if evaluation.estimates.iter().any(|e| e.id == row) {
            ids.insert(row.into());
            found = true;
        }
        if !found {
            return Err(StoreError::NotFound(format!("analysis row {row}")));
        }
        loop {
            let before = ids.len();
            for e in &evaluation.estimates {
                if ids.contains(&e.id) {
                    ids.extend(e.dependencies.iter().cloned());
                }
            }
            if ids.len() == before {
                break;
            }
        }
        let estimates: Vec<_> = evaluation
            .estimates
            .iter()
            .filter(|e| ids.contains(&e.id) || e.work_items.iter().any(|w| work_ids.contains(w)))
            .cloned()
            .collect();
        for e in &estimates {
            work_ids.extend(e.work_items.iter().copied());
            ids.insert(e.id.clone());
        }
        let mut observations = BTreeMap::new();
        for identity in &ids {
            for o in evaluation.observations.get(identity).into_iter().flatten() {
                observations.insert(o.attempt, o.clone());
            }
        }
        let mut attempts = Vec::new();
        for &work in &work_ids {
            for attempt in store.attempts::<Value>(work)? {
                attempts.push(AttemptInspection {
                    id: attempt.id,
                    work_item: work,
                    repetition: attempt.sample_index,
                    state: attempt.state,
                    started_at: attempt.started_at,
                    artifact_state: attempt.artifact_state,
                    record: attempt.record,
                    artifacts: store.artifacts(attempt.id)?,
                });
            }
        }
        let runs: Vec<_> = store
            .runs(snapshot.plan.selection.experiment)?
            .into_iter()
            .filter(|r| {
                estimates.iter().any(|e| e.run_ids.contains(&r.id))
                    || store
                        .work_items(r.id)
                        .is_ok_and(|ws| ws.iter().any(|w| work_ids.contains(&w.id)))
            })
            .collect();
        if store.generation()? != snapshot.generation {
            return Err(StoreError::Expired);
        }
        Ok(Inspection {
            estimates: Page::from_items(estimates, paging.offset, paging.limit),
            observations: Page::from_items(observations.into_values(), paging.offset, paging.limit),
            attempts: Page::from_items(attempts, paging.offset, paging.limit),
            runs,
        })
    }
}

/// Linked descriptors construct decoders without creating suites, sessions, or tools.
pub struct AdapterRegistration {
    factory: fn() -> Box<dyn ExperimentAdapter>,
}
impl AdapterRegistration {
    pub const fn for_procedure<P: StoredProcedure>() -> Self
    where
        P::Input: Send + Sync,
        P::Output: Send + Sync,
    {
        Self {
            factory: make_adapter::<P>,
        }
    }
}
fn make_adapter<P: StoredProcedure>() -> Box<dyn ExperimentAdapter>
where
    P::Input: Send + Sync,
    P::Output: Send + Sync,
{
    Box::new(TypedAdapter::<P> {
        catalog: Cache::new(4),
        frames: Cache::new(3),
        evaluations: Cache::new(8),
        input_pages: Cache::new(8),
    })
}
inventory::collect!(AdapterRegistration);

pub struct AnalysisRegistry {
    adapters: BTreeMap<String, Box<dyn ExperimentAdapter>>,
}
impl AnalysisRegistry {
    fn shared() -> Result<Arc<Self>> {
        static REGISTRY: OnceLock<Arc<AnalysisRegistry>> = OnceLock::new();
        if let Some(registry) = REGISTRY.get() {
            return Ok(registry.clone());
        }
        let registry = Arc::new(Self::discover()?);
        let _ = REGISTRY.set(registry);
        Ok(REGISTRY.get().unwrap().clone())
    }
    pub fn discover() -> Result<Self> {
        let mut adapters = BTreeMap::new();
        for registration in inventory::iter::<AdapterRegistration> {
            let adapter = (registration.factory)();
            let contract = adapter.contract();
            let metrics = adapter.metrics();
            let mut paths = BTreeSet::new();
            for metric in &metrics {
                if metric.path.is_empty()
                    || !paths.insert(&metric.path)
                    || !metric.repetitions.contains(&metric.default_repetitions)
                    || !metric.runs.contains(&metric.default_runs)
                    || !metric.inputs.contains(&metric.default_inputs)
                    || metric.reference.is_some_and(|v| !v.is_finite())
                {
                    return Err(StoreError::Invalid(format!(
                        "Invalid metric descriptor for {}: {}",
                        contract.procedure, metric.path
                    )));
                }
            }
            if let Some(default) = adapter.default_metric()
                && !paths.contains(&default)
            {
                return Err(StoreError::Invalid(format!(
                    "Unknown default metric for {}: {default}",
                    contract.procedure
                )));
            }
            let fields = adapter.fields();
            if fields
                .iter()
                .map(|f| &f.path)
                .collect::<BTreeSet<_>>()
                .len()
                != fields.len()
            {
                return Err(StoreError::Invalid(format!(
                    "Duplicate input fields for {}",
                    contract.procedure
                )));
            }
            let key = crate::store::encode(&contract)?;
            if adapters.insert(key, adapter).is_some() {
                return Err(StoreError::Invalid(format!(
                    "Conflicting adapter registration: {}",
                    contract.procedure
                )));
            }
        }
        Ok(Self { adapters })
    }
    fn adapter(
        &self,
        store: &SqliteStore,
        experiment: ExperimentId,
    ) -> Result<&dyn ExperimentAdapter> {
        let contract = store.describe_experiment(experiment)?.contract;
        self.adapters
            .get(&crate::store::encode(&contract)?)
            .map(|a| a.as_ref())
            .ok_or_else(|| StoreError::Unsupported(contract.procedure))
    }
}

/// Local server boundary. All public request/result values are serializable;
/// the UI can wrap these methods in Dioxus server functions without knowing P.
pub struct Backend {
    pub store: SqliteStore,
    registry: Arc<AnalysisRegistry>,
}
impl Backend {
    pub fn new(store: SqliteStore) -> Result<Self> {
        Ok(Self {
            store,
            registry: AnalysisRegistry::shared()?,
        })
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::new(SqliteStore::open(path)?)
    }
    pub fn experiments(&self) -> Result<Vec<ExperimentSummary>> {
        self.store.experiments()
    }
    pub fn describe(&self, id: ExperimentId) -> Result<ExperimentDescription> {
        let experiment = self.store.describe_experiment(id)?;
        let versions = self.store.definitions(id)?;
        let (metrics, fields, operations, unsupported) =
            match self.registry.adapter(&self.store, id) {
                Ok(a) => (
                    a.stored_metrics(&self.store, id)?,
                    a.fields(),
                    vec!["history", "changes", "scaling", "variation", "coverage"]
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                    None,
                ),
                Err(StoreError::Unsupported(reason)) => (vec![], vec![], vec![], Some(reason)),
                Err(error) => return Err(error),
            };
        let default_metric = self
            .registry
            .adapter(&self.store, id)
            .ok()
            .and_then(|a| a.default_metric());
        Ok(ExperimentDescription {
            default_metric,
            experiment,
            versions,
            metrics,
            fields,
            run_fields: self.store.run_fields(id)?,
            operations,
            unsupported,
        })
    }
    pub fn inputs(
        &self,
        s: &ResultSelection,
        search: &str,
        paging: Paging,
    ) -> Result<Page<InputDescription>> {
        self.registry
            .adapter(&self.store, s.experiment)?
            .inputs(&self.store, s, search, paging)
    }
    pub fn groups(&self, s: &ResultSelection, field: &str) -> Result<Vec<(Value, usize)>> {
        self.registry
            .adapter(&self.store, s.experiment)?
            .groups(&self.store, s, field)
    }
    pub fn revisions(
        &self,
        experiment: ExperimentId,
        query: &RevisionQuery,
    ) -> Result<Vec<RevisionDescription>> {
        Ok(self
            .store
            .revisions(experiment, query)?
            .into_iter()
            .map(|r| RevisionDescription {
                id: r.id().id().clone(),
                committed_at: *r.committed_at(),
                author: r.author().clone(),
                message: r.commit_message().clone(),
            })
            .collect())
    }
    pub fn runs(&self, experiment: ExperimentId) -> Result<Vec<RunSummary>> {
        self.store.runs(experiment)
    }
    pub fn analyze(&self, plan: &AnalysisPlan) -> Result<AnalysisResponse> {
        self.registry
            .adapter(&self.store, plan.selection.experiment)?
            .analyze(&self.store, plan)
    }
    pub fn inspect(&self, snapshot: &Snapshot, row: &str, paging: Paging) -> Result<Inspection> {
        self.registry
            .adapter(&self.store, snapshot.plan.selection.experiment)?
            .inspect(&self.store, snapshot, row, paging)
    }
    pub fn inspect_run(&self, run: RunId) -> Result<RunInspection> {
        Ok(RunInspection {
            run: self.store.run(run)?,
            revisions: self.store.revision_statuses(run)?,
            work_items: self.store.work_items(run)?,
        })
    }
    pub fn inspect_work_item(
        &self,
        work: WorkItemId,
        paging: Paging,
    ) -> Result<Page<AttemptInspection>> {
        let mut attempts = Vec::new();
        for a in self.store.attempts::<Value>(work)? {
            attempts.push(AttemptInspection {
                id: a.id,
                work_item: work,
                repetition: a.sample_index,
                state: a.state,
                started_at: a.started_at,
                artifact_state: a.artifact_state,
                record: a.record,
                artifacts: self.store.artifacts(a.id)?,
            });
        }
        Ok(Page::from_items(attempts, paging.offset, paging.limit))
    }
    pub fn read_artifact(&self, id: ArtifactId) -> Result<Vec<u8>> {
        self.store.read_artifact(id)
    }
}
