use super::*;
use crate::experiment::{BenchmarkIn, InputDescription, Page};
use crate::{experiment::Version, src_manager::git::history::Commit};
use rusqlite::OptionalExtension;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn state(value: String) -> Result<State> {
    decode(&format!("\"{value}\""))
}

impl SqliteStore {
    pub fn experiments(&self) -> Result<Vec<ExperimentSummary>> {
        let mut stmt=self.connection.prepare("SELECT e.id,e.key,e.name,e.description,e.repository,e.contract,e.created_at,(SELECT max(a.started_at) FROM attempts a JOIN work_items w ON w.id=a.work_item JOIN runs r ON r.id=w.run WHERE r.experiment=e.id),(SELECT count(*) FROM attempts a JOIN work_items w ON w.id=a.work_item JOIN runs r ON r.id=w.run WHERE r.experiment=e.id AND a.outcome='success'),(SELECT count(DISTINCT w.revision) FROM attempts a JOIN work_items w ON w.id=a.work_item JOIN runs r ON r.id=w.run WHERE r.experiment=e.id AND a.outcome='success') FROM experiments e ORDER BY e.id")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                ExperimentSpec {
                    key: r.get(1)?,
                    name: r.get(2)?,
                    description: r.get(3)?,
                    repository: r.get(4)?,
                },
                r.get::<_, String>(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get::<_, i64>(8)? as usize,
                r.get::<_, i64>(9)? as usize,
            ))
        })?;
        rows.map(|row| {
            let (
                id,
                spec,
                contract,
                created_at,
                last_measured_at,
                successful_attempts,
                measured_revisions,
            ) = row?;
            Ok(ExperimentSummary {
                id: ExperimentId(id),
                spec,
                contract: decode(&contract)?,
                created_at,
                last_measured_at,
                successful_attempts,
                measured_revisions,
            })
        })
        .collect()
    }
    pub fn describe_experiment(&self, id: ExperimentId) -> Result<ExperimentSummary> {
        self.experiments()?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| StoreError::NotFound(format!("experiment {id}")))
    }
    pub fn validate<P: StoredProcedure>(&self, id: ExperimentId) -> Result<()> {
        let contract: String = self
            .connection
            .query_row("SELECT contract FROM experiments WHERE id=?", [id.0], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("experiment {id}")))?;
        if decode::<Contract>(&contract)? == contract_for::<P>() {
            Ok(())
        } else {
            Err(StoreError::Unsupported(contract))
        }
    }
    pub fn definitions(&self, experiment: ExperimentId) -> Result<Vec<DefinitionSummary>> {
        let mut stmt=self.connection.prepare("SELECT d.id,d.major,d.minor,(SELECT count(*) FROM suite_inputs WHERE definition=d.id),(SELECT count(*) FROM attempts a JOIN work_items w ON w.id=a.work_item JOIN runs r ON r.id=w.run WHERE r.definition=d.id AND a.outcome='success') FROM definitions d WHERE d.experiment=? ORDER BY d.major,d.minor")?;
        Ok(stmt
            .query_map([experiment.0], |r| {
                Ok(DefinitionSummary {
                    id: DefinitionId(r.get(0)?),
                    experiment,
                    version: Version::new(r.get(1)?, r.get(2)?),
                    input_count: r.get::<_, i64>(3)? as usize,
                    successful_attempts: r.get::<_, i64>(4)? as usize,
                })
            })?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn target_definition(
        &self,
        experiment: ExperimentId,
        version: Option<Version>,
    ) -> Result<DefinitionSummary> {
        self.definitions(experiment)?
            .into_iter()
            .rev()
            .find(|d| version.is_none_or(|v| d.version == v))
            .ok_or_else(|| StoreError::NotFound("suite version".into()))
    }
    pub(crate) fn typed_inputs<P: StoredProcedure>(
        &self,
        definition: DefinitionId,
        selection: &InputSelection,
    ) -> Result<BTreeMap<InputId, P::Input>> {
        let fields = P::Input::fields();
        if let InputSelection::Matching { filters, .. } = selection {
            for filter in filters {
                filter.validate(&fields).map_err(invalid)?;
            }
        }
        let mut stmt=self.connection.prepare("SELECT i.id,i.payload FROM suite_inputs s JOIN inputs i ON i.id=s.input WHERE s.definition=? ORDER BY s.position")?;
        let mut inputs = BTreeMap::new();
        for row in stmt.query_map([definition.0], |r| {
            Ok((InputId(r.get(0)?), r.get::<_, String>(1)?))
        })? {
            let (id, json) = row?;
            inputs.insert(id, decode::<P::Input>(&json)?);
        }
        let requested = match selection {
            InputSelection::Ids { ids } => Some(ids),
            InputSelection::Matching { excluded, .. } => Some(excluded),
            _ => None,
        };
        if let Some(ids) = requested
            && (ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
                || ids.iter().any(|id| !inputs.contains_key(id)))
        {
            return Err(invalid("Unknown or duplicate selected/excluded input"));
        }
        inputs.retain(|id, input| match selection {
            InputSelection::All => true,
            InputSelection::Ids { ids } => ids.contains(id),
            InputSelection::Matching {
                filters,
                search,
                excluded,
            } => {
                !excluded.contains(id) && {
                    let p = input.parameters();
                    let text = format!(
                        "{} {}",
                        input.label(),
                        serde_json::to_string(&p).unwrap_or_default()
                    )
                    .to_lowercase();
                    text.contains(&search.to_lowercase())
                        && filters.iter().all(|f| f.matches(p.get(&f.field)))
                }
            }
        });
        Ok(inputs)
    }
    fn input_order(&self, definition: DefinitionId) -> Result<Vec<InputId>> {
        let mut stmt = self
            .connection
            .prepare("SELECT input FROM suite_inputs WHERE definition=? ORDER BY position")?;
        Ok(stmt
            .query_map([definition.0], |row| Ok(InputId(row.get(0)?)))?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn inputs<P: StoredProcedure>(
        &self,
        selection: &ResultSelection,
        search: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<InputDescription>> {
        self.validate::<P>(selection.experiment)?;
        let def = self.target_definition(selection.experiment, selection.version)?;
        let search = search.to_lowercase();
        let inputs = self.typed_inputs::<P>(def.id, &selection.inputs)?;
        let descriptions = self
            .input_order(def.id)?
            .into_iter()
            .filter_map(|id| {
                inputs.get(&id).map(|input| InputDescription {
                    id,
                    label: input.label(),
                    parameters: input.parameters(),
                })
            })
            .filter(|d| {
                format!(
                    "{} {}",
                    d.label,
                    serde_json::to_string(&d.parameters).unwrap_or_default()
                )
                .to_lowercase()
                .contains(&search)
            });
        Ok(Page::from_items(descriptions, offset, limit))
    }
    pub fn groups<P: StoredProcedure>(
        &self,
        selection: &ResultSelection,
        field: &str,
    ) -> Result<Vec<(serde_json::Value, usize)>> {
        let descriptor = P::Input::fields()
            .into_iter()
            .find(|f| f.path == field && f.groupable)
            .ok_or_else(|| invalid("Unknown or ungroupable field"))?;
        let inputs = self.inputs::<P>(selection, "", 0, usize::MAX)?;
        let mut groups = BTreeMap::<String, (serde_json::Value, usize)>::new();
        for input in inputs.items {
            let value = input
                .parameters
                .get(&descriptor.path)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            groups.entry(encode(&value)?).or_insert((value, 0)).1 += 1;
        }
        Ok(groups.into_values().collect())
    }
    pub fn runs(&self, experiment: ExperimentId) -> Result<Vec<RunSummary>> {
        let mut stmt=self.connection.prepare("SELECT r.id,r.definition,d.major,d.minor,r.plan,r.partition_key,r.state,r.started_at,r.finished_at FROM runs r JOIN definitions d ON d.id=r.definition WHERE r.experiment=? ORDER BY r.id")?;
        let rows = stmt.query_map([experiment.0], |r| {
            Ok((
                RunId(r.get(0)?),
                DefinitionId(r.get(1)?),
                Version::new(r.get(2)?, r.get(3)?),
                r.get::<_, String>(4)?,
                r.get(5)?,
                r.get::<_, String>(6)?,
                r.get(7)?,
                r.get(8)?,
            ))
        })?;
        rows.map(|row| {
            let (id, definition, version, plan, partition, status, started_at, finished_at) = row?;
            let plan: RunPlan = decode(&plan)?;
            Ok(RunSummary {
                id,
                experiment,
                definition,
                version,
                partition,
                history: plan.history,
                environment: plan.environment,
                options: plan.options,
                inputs: plan.resolved_inputs,
                revisions: plan.revisions.iter().map(|r| r.id().id().clone()).collect(),
                purpose: plan.purpose,
                state: state(status)?,
                started_at,
                finished_at,
                repetitions: plan.repetitions,
            })
        })
        .collect()
    }
    pub fn run_parameters(run: &RunSummary) -> BTreeMap<String, serde_json::Value> {
        fn visit(
            out: &mut BTreeMap<String, serde_json::Value>,
            prefix: &str,
            value: &serde_json::Value,
        ) {
            if let Some(object) = value.as_object() {
                for (key, value) in object {
                    visit(
                        out,
                        &format!("{prefix}/{}", key.replace('~', "~0").replace('/', "~1")),
                        value,
                    );
                }
            } else if value.is_number() || value.is_boolean() || value.is_string() {
                out.insert(format!("option:{prefix}"), value.clone());
            }
        }
        let mut out = BTreeMap::from([
            (
                "environment.machine".into(),
                serde_json::json!(run.environment.machine),
            ),
            (
                "environment.os".into(),
                serde_json::json!(run.environment.os),
            ),
            (
                "environment.arch".into(),
                serde_json::json!(run.environment.arch),
            ),
        ]);
        for (key, value) in &run.environment.attributes {
            out.insert(format!("environment.{key}"), serde_json::json!(value));
        }
        visit(&mut out, "", &run.options);
        out
    }
    pub fn run_fields(
        &self,
        experiment: ExperimentId,
    ) -> Result<Vec<crate::experiment::InputField>> {
        use crate::experiment::{FieldKind, InputField};
        let mut fields = BTreeMap::<String, Option<FieldKind>>::new();
        for run in self.runs(experiment)? {
            for (path, value) in Self::run_parameters(&run) {
                let kind = if value.is_number() {
                    FieldKind::Number
                } else if value.is_boolean() {
                    FieldKind::Boolean
                } else {
                    FieldKind::Text
                };
                fields
                    .entry(path)
                    .and_modify(|previous| {
                        if *previous != Some(kind) {
                            *previous = None;
                        }
                    })
                    .or_insert(Some(kind));
            }
        }
        Ok(fields
            .into_iter()
            .filter_map(|(path, kind)| kind.map(|kind| InputField::new(&path, &path, kind)))
            .collect())
    }
    fn run_plan(&self, run: RunId) -> Result<RunPlan> {
        let json: String =
            self.connection
                .query_row("SELECT plan FROM runs WHERE id=?", [run.0], |r| r.get(0))?;
        decode(&json)
    }
    pub fn revisions(
        &self,
        experiment: ExperimentId,
        query: &RevisionQuery,
    ) -> Result<Vec<Commit>> {
        let mut commits = BTreeMap::new();
        let mut positions = BTreeMap::new();
        let mut edges = BTreeMap::<String, BTreeSet<String>>::new();
        let mut histories = BTreeSet::new();
        for run in self
            .runs(experiment)?
            .into_iter()
            .filter(|r| r.purpose == Purpose::Measurement)
        {
            histories.insert(run.history.clone());
            let plan = self.run_plan(run.id)?;
            let mut prev = None;
            for commit in plan.revisions {
                let id = commit.id().id().clone();
                let n = positions.len();
                positions.entry(id.clone()).or_insert(n);
                if let Some(prev) = prev {
                    edges.entry(prev).or_default().insert(id.clone());
                }
                prev = Some(id.clone());
                commits.insert(id, commit);
            }
        }
        if let RevisionQuery::Only { ids } = query {
            let mut seen = BTreeSet::new();
            return ids
                .iter()
                .map(|id| {
                    if !seen.insert(id) {
                        return Err(invalid("Duplicate selected revision"));
                    }
                    commits
                        .get(id)
                        .cloned()
                        .ok_or_else(|| invalid(format!("Unknown revision {id}")))
                })
                .collect();
        }
        if histories.len() > 1 {
            return Err(invalid(
                "Multiple histories require an explicit revision order",
            ));
        }
        let mut incoming: BTreeMap<_, usize> = commits.keys().map(|id| (id.clone(), 0)).collect();
        for children in edges.values() {
            for child in children {
                *incoming.get_mut(child).unwrap() += 1;
            }
        }
        let mut ready: BTreeSet<_> = incoming
            .iter()
            .filter(|(_, n)| **n == 0)
            .map(|(id, _)| (positions[id], id.clone()))
            .collect();
        let mut ordered = Vec::new();
        while let Some((_, id)) = ready.pop_first() {
            ordered.push(commits[&id].clone());
            for child in edges.get(&id).into_iter().flatten() {
                let n = incoming.get_mut(child).unwrap();
                *n -= 1;
                if *n == 0 {
                    ready.insert((positions[child], child.clone()));
                }
            }
        }
        if ordered.len() != commits.len() {
            return Err(invalid(
                "Conflicting revision orders; select an explicit order",
            ));
        }
        if let RevisionQuery::Range { start, end } = query {
            let position = |id: &str| {
                ordered
                    .iter()
                    .position(|r| r.id().id() == id)
                    .ok_or_else(|| invalid(format!("Unknown revision {id}")))
            };
            let start = start.as_deref().map(position).transpose()?.unwrap_or(0);
            let end = end
                .as_deref()
                .map(position)
                .transpose()?
                .unwrap_or_else(|| ordered.len().saturating_sub(1));
            if start > end {
                return Err(invalid("Reversed revision range"));
            }
            return Ok(ordered
                .into_iter()
                .skip(start)
                .take(end - start + 1)
                .collect());
        }
        Ok(ordered)
    }
    /// Successful payloads across the experiment, including unselected revisions.
    /// Used to discover named metric maps without making absent values into zeros.
    pub(crate) fn successful_outputs<P: StoredProcedure>(
        &self,
        experiment: ExperimentId,
    ) -> Result<Vec<P::Output>> {
        self.validate::<P>(experiment)?;
        let mut statement = self.connection.prepare(
            "SELECT a.record FROM attempts a JOIN work_items w ON w.id=a.work_item JOIN runs r ON r.id=w.run WHERE r.experiment=? AND coalesce(json_extract(r.plan,'$.purpose'),'measurement')='measurement' AND a.outcome='success' ORDER BY a.id"
        )?;
        let records = statement.query_map([experiment.0], |row| row.get::<_, String>(0))?;
        let mut outputs = Vec::new();
        for record in records {
            let record: crate::experiment::AttemptRecord<P::Output> = decode(&record?)?;
            if let crate::experiment::AttemptOutcome::Success(output) = record.outcome {
                outputs.push(output);
            }
        }
        Ok(outputs)
    }

    pub fn frame<P: StoredProcedure>(
        &self,
        selection: &ResultSelection,
    ) -> Result<ResultFrame<P::Input, P::Output>> {
        // A transaction gives all parts of a frame one SQLite snapshot.
        let tx = self.connection.unchecked_transaction()?;
        self.validate::<P>(selection.experiment)?;
        let generation = self.generation()?;
        let definition = self.target_definition(selection.experiment, selection.version)?;
        let inputs = self.typed_inputs::<P>(definition.id, &selection.inputs)?;
        let input_order = self
            .input_order(definition.id)?
            .into_iter()
            .filter(|id| inputs.contains_key(id))
            .collect();
        let descriptions = inputs
            .iter()
            .map(|(&id, input)| {
                (
                    id,
                    InputDescription {
                        id,
                        label: input.label(),
                        parameters: input.parameters(),
                    },
                )
            })
            .collect();
        let revisions = self.revisions(selection.experiment, &selection.revisions)?;
        let revision_ids: BTreeSet<_> = revisions.iter().map(|r| r.id().id().as_str()).collect();
        let mut runs: Vec<_> = self
            .runs(selection.experiment)?
            .into_iter()
            .filter(|r| {
                r.purpose == Purpose::Measurement
                    && match selection.sources {
                        VersionSources::Exact => r.version == definition.version,
                        VersionSources::CompatibleSupersets => {
                            r.version.major() == definition.version.major()
                                && r.version.minor() >= definition.version.minor()
                        }
                    }
            })
            .collect();
        let fields = self.run_fields(selection.experiment)?;
        for filter in &selection.run_filters {
            filter.validate(&fields).map_err(invalid)?;
        }
        runs.retain(|run| {
            let parameters = Self::run_parameters(run);
            selection
                .run_filters
                .iter()
                .all(|f| f.matches(parameters.get(&f.field)))
        });
        if let RunSelection::Explicit { ids } = &selection.runs {
            if ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
                || ids.iter().any(|id| !runs.iter().any(|r| r.id == *id))
            {
                return Err(invalid("Unknown, incompatible, or duplicate selected run"));
            }
            runs.retain(|r| ids.contains(&r.id));
        }
        let partitions: BTreeSet<_> = runs.iter().map(|r| r.partition.clone()).collect();
        match &selection.partitions {
            PartitionSelection::Automatic if partitions.len() > 1 => {
                return Err(invalid(
                    "Select an environment/configuration partition, or explicitly select all",
                ));
            }
            PartitionSelection::Only { ids } => {
                if ids.iter().any(|id| !partitions.contains(id)) {
                    return Err(invalid("Unknown partition"));
                }
                runs.retain(|r| ids.contains(&r.partition));
            }
            _ => (),
        }
        let mut candidates = BTreeMap::<(String, String, InputId), Vec<WorkItemSummary>>::new();
        for run in &runs {
            for work in self.work_items(run.id)? {
                if inputs.contains_key(&work.input) && revision_ids.contains(work.revision.as_str())
                {
                    candidates
                        .entry((run.partition.clone(), work.revision.clone(), work.input))
                        .or_default()
                        .push(work);
                }
            }
        }
        let mut work_items = Vec::new();
        let mut newer_unfinished = Vec::new();
        for mut items in candidates.into_values() {
            items.sort_by_key(|w| w.id);
            if matches!(selection.runs, RunSelection::LatestCompleted) {
                let chosen = items
                    .iter()
                    .rposition(|w| w.state.terminal())
                    .unwrap_or(items.len() - 1);
                let id = items[chosen].id;
                newer_unfinished.extend(
                    items
                        .iter()
                        .filter(|w| w.id > id && !w.state.terminal())
                        .cloned(),
                );
                work_items.push(items.remove(chosen));
            } else {
                work_items.extend(items);
            }
        }
        let mut attempts = Vec::new();
        for work in &work_items {
            attempts.extend(self.attempts::<P::Output>(work.id)?);
        }
        tx.commit()?;
        Ok(ResultFrame {
            generation,
            definition,
            revisions,
            input_order,
            inputs,
            descriptions,
            runs,
            work_items,
            attempts,
            newer_unfinished,
        })
    }
    pub fn revision_statuses(&self, run: RunId) -> Result<Vec<RevisionStatus>> {
        let mut stmt = self.connection.prepare(
            "SELECT revision,state,reason FROM run_revisions WHERE run=? ORDER BY position",
        )?;
        stmt.query_map([run.0], |r| {
            Ok((r.get(0)?, r.get::<_, String>(1)?, r.get(2)?))
        })?
        .map(|row| {
            let (revision, status, reason) = row?;
            Ok(RevisionStatus {
                revision,
                state: state(status)?,
                reason,
            })
        })
        .collect()
    }
    pub fn run(&self, id: RunId) -> Result<RunSummary> {
        let experiment = self
            .connection
            .query_row("SELECT experiment FROM runs WHERE id=?", [id.0], |r| {
                Ok(ExperimentId(r.get(0)?))
            })
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("run {id}")))?;
        self.runs(experiment)?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| StoreError::NotFound(format!("run {id}")))
    }
    pub fn work_items(&self, run: RunId) -> Result<Vec<WorkItemSummary>> {
        let mut stmt=self.connection.prepare("SELECT id,revision,input,state,reason,planned_repetitions FROM work_items WHERE run=? ORDER BY id")?;
        stmt.query_map([run.0], |r| {
            Ok((
                WorkItemId(r.get(0)?),
                r.get(1)?,
                InputId(r.get(2)?),
                r.get::<_, String>(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?
        .map(|row| {
            let (id, revision, input, status, reason, planned_repetitions) = row?;
            Ok(WorkItemSummary {
                id,
                run,
                revision,
                input,
                state: state(status)?,
                reason,
                planned_repetitions,
            })
        })
        .collect()
    }
    pub(crate) fn attempts<O: serde::de::DeserializeOwned>(
        &self,
        work: WorkItemId,
    ) -> Result<Vec<StoredAttempt<O>>> {
        let mut stmt=self.connection.prepare("SELECT id,sample,state,started_at,record,artifact_state FROM attempts WHERE work_item=? ORDER BY sample,id")?;
        stmt.query_map([work.0], |r| {
            Ok((
                AttemptId(r.get(0)?),
                r.get(1)?,
                r.get::<_, String>(2)?,
                r.get(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?
        .map(|row| {
            let (id, sample_index, status, started_at, record, artifact_state) = row?;
            Ok(StoredAttempt {
                id,
                work_item: work,
                sample_index,
                state: state(status)?,
                artifact_state: decode(&format!("\"{artifact_state}\""))?,
                started_at,
                record: record.map(|s| decode(&s)).transpose()?,
            })
        })
        .collect()
    }
    pub fn artifacts(&self, attempt: AttemptId) -> Result<Vec<Artifact>> {
        let mut stmt = self
            .connection
            .prepare("SELECT id,name,path FROM artifacts WHERE attempt=? ORDER BY id")?;
        Ok(stmt
            .query_map([attempt.0], |r| {
                let path = PathBuf::from(r.get::<_, String>(2)?);
                Ok(Artifact {
                    id: ArtifactId(r.get(0)?),
                    attempt,
                    name: r.get(1)?,
                    available: path.is_file(),
                })
            })?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn artifact(&self, id: ArtifactId) -> Result<Artifact> {
        self.connection
            .query_row(
                "SELECT attempt,name,path FROM artifacts WHERE id=?",
                [id.0],
                |r| {
                    let path = PathBuf::from(r.get::<_, String>(2)?);
                    Ok(Artifact {
                        id,
                        attempt: AttemptId(r.get(0)?),
                        name: r.get(1)?,
                        available: path.is_file(),
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("artifact {id}")))
    }

    pub fn read_artifact(&self, id: ArtifactId) -> Result<Vec<u8>> {
        let path: String = self
            .connection
            .query_row("SELECT path FROM artifacts WHERE id=?", [id.0], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("artifact {id}")))?;
        Ok(std::fs::read(path)?)
    }
}
