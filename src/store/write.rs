use super::*;
use crate::experiment::{AttemptRecord, SuiteDefinition, Version};
use rusqlite::{OptionalExtension, params};
use std::{collections::BTreeSet, path::Path};

impl SqliteStore {
    pub fn register_run<P: StoredProcedure>(
        &mut self,
        experiment: &ExperimentSpec,
        definition: &SuiteDefinition<P::Input>,
        plan: &RunPlan,
    ) -> Result<RegisteredRun> {
        if experiment.key.is_empty()
            || experiment.repository.is_empty()
            || plan.history.is_empty()
            || plan.repetitions == 0
        {
            return Err(invalid(
                "Experiment key, repository, history, and positive repetitions are required",
            ));
        }
        if definition.procedure().name() != P::STORAGE_KEY {
            return Err(invalid(
                "Definition procedure does not match its storage adapter",
            ));
        }
        let mut revisions = BTreeSet::new();
        if plan
            .revisions
            .iter()
            .any(|r| !revisions.insert(r.id().id()))
        {
            return Err(invalid("Duplicate revisions"));
        }
        let indices = plan
            .input_indices
            .clone()
            .unwrap_or_else(|| (0..definition.inputs().len()).collect());
        let mut seen = BTreeSet::new();
        if indices
            .iter()
            .any(|i| *i >= definition.inputs().len() || !seen.insert(*i))
        {
            return Err(invalid("Invalid or duplicate input indices"));
        }
        let tx = self.connection.transaction()?;
        let contract = encode(&contract_for::<P>())?;
        let existing: Option<(i64, String)> = tx
            .query_row(
                "SELECT id,contract FROM experiments WHERE repository=? AND key=?",
                params![experiment.repository, experiment.key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let exp = if let Some((id, stored)) = existing {
            if stored != contract {
                return Err(StoreError::Unsupported("Experiment contract changed; use a new experiment key or an explicit migration".into()));
            }
            id
        } else {
            tx.execute("INSERT INTO experiments(key,name,description,repository,contract,created_at) VALUES(?,?,?,?,?,?)", params![experiment.key,experiment.name,experiment.description,experiment.repository,contract,now()])?;
            tx.last_insert_rowid()
        };
        let mut known = Vec::<(InputId, P::Input)>::new();
        {
            let mut stmt = tx.prepare(
                "SELECT id,payload FROM inputs WHERE procedure=? AND schema_version=? ORDER BY id",
            )?;
            for row in stmt.query_map(params![P::STORAGE_KEY, P::INPUT_SCHEMA], |r| {
                Ok((InputId(r.get(0)?), r.get::<_, String>(1)?))
            })? {
                let (id, payload) = row?;
                known.push((id, decode(&payload)?));
            }
        }
        let mut input_ids = Vec::new();
        for input in definition.inputs() {
            let id = if let Some((id, _)) = known.iter().find(|(_, value)| value == input) {
                *id
            } else {
                tx.execute(
                    "INSERT INTO inputs(procedure,schema_version,payload) VALUES(?,?,?)",
                    params![P::STORAGE_KEY, P::INPUT_SCHEMA, encode(input)?],
                )?;
                let id = InputId(tx.last_insert_rowid());
                known.push((id, input.clone()));
                id
            };
            input_ids.push(id);
        }
        let wanted: BTreeSet<_> = input_ids.iter().copied().collect();
        let defs = {
            let mut stmt=tx.prepare("SELECT id,major,minor FROM definitions WHERE experiment=? ORDER BY major DESC,minor DESC")?;
            stmt.query_map([exp], |r| {
                Ok((DefinitionId(r.get(0)?), Version::new(r.get(1)?, r.get(2)?)))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut existing_def = None;
        let mut latest_set = None;
        for (id, version) in &defs {
            let mut stmt = tx.prepare("SELECT input FROM suite_inputs WHERE definition=?")?;
            let ids = stmt
                .query_map([id.0], |r| Ok(InputId(r.get(0)?)))?
                .collect::<std::result::Result<BTreeSet<_>, _>>()?;
            if ids == wanted {
                existing_def = Some((*id, *version));
                break;
            }
            if latest_set.is_none() {
                latest_set = Some((*version, ids));
            }
        }
        let (def, version) = if let Some(found) = existing_def {
            found
        } else {
            let version = match latest_set {
                None => Version::default(),
                Some((v, ids)) if wanted.is_superset(&ids) => {
                    Version::new(*v.major(), v.minor() + 1)
                }
                Some((v, _)) => Version::new(v.major() + 1, 0),
            };
            tx.execute(
                "INSERT INTO definitions(experiment,major,minor) VALUES(?,?,?)",
                params![exp, version.major(), version.minor()],
            )?;
            let id = DefinitionId(tx.last_insert_rowid());
            for (pos, input) in input_ids.iter().enumerate() {
                tx.execute(
                    "INSERT INTO suite_inputs VALUES(?,?,?)",
                    params![id.0, input.0, pos as i64],
                )?;
            }
            (id, version)
        };
        // The plan preserves the invocation's order; the definition keeps its original display order.
        for revision in &plan.revisions {
            let payload = encode(revision)?;
            let old: Option<String> = tx
                .query_row(
                    "SELECT payload FROM revisions WHERE experiment=? AND revision=?",
                    params![exp, revision.id().id()],
                    |r| r.get(0),
                )
                .optional()?;
            if old.as_ref().is_some_and(|s| s != &payload) {
                return Err(invalid("Conflicting commit metadata"));
            }
            tx.execute(
                "INSERT OR IGNORE INTO revisions VALUES(?,?,?)",
                params![exp, revision.id().id(), payload],
            )?;
        }
        let mut resolved_plan = plan.clone();
        resolved_plan.resolved_inputs = indices.iter().map(|&i| input_ids[i]).collect();
        let partition = fingerprint(&(&plan.environment, &plan.options))?;
        tx.execute("INSERT INTO runs(experiment,definition,plan,partition_key,state,started_at) VALUES(?,?,?,?,?,?)",params![exp,def.0,encode(&resolved_plan)?,partition,"running",now()])?;
        let run = RunId(tx.last_insert_rowid());
        let mut work_items = std::collections::BTreeMap::new();
        for (position, revision) in plan.revisions.iter().enumerate() {
            tx.execute(
                "INSERT INTO run_revisions(run,revision,position,state) VALUES(?,?,?,?)",
                params![run.0, revision.id().id(), position as i64, "planned"],
            )?;
            let mut work = std::collections::BTreeMap::new();
            for &index in &indices {
                let input = input_ids[index];
                tx.execute("INSERT INTO work_items(run,revision,input,state,planned_repetitions) VALUES(?,?,?,?,?)",params![run.0,revision.id().id(),input.0,"planned",plan.repetitions])?;
                work.insert(input, WorkItemId(tx.last_insert_rowid()));
            }
            work_items.insert(revision.id().id().clone(), work);
        }
        changed(&tx)?;
        tx.commit()?;
        Ok(RegisteredRun {
            run,
            experiment: ExperimentId(exp),
            definition: DefinitionSummary {
                id: def,
                experiment: ExperimentId(exp),
                version,
                input_count: input_ids.len(),
                successful_attempts: 0,
            },
            inputs: input_ids,
            work_items,
        })
    }

    pub fn start_attempt(&mut self, work: WorkItemId, sample: u32) -> Result<AttemptId> {
        let tx = self.connection.transaction()?;
        let (state,count,run_state):(String,u32,String)=tx.query_row("SELECT w.state,w.planned_repetitions,r.state FROM work_items w JOIN runs r ON r.id=w.run WHERE w.id=?",[work.0],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if !matches!(state.as_str(), "planned" | "running")
            || run_state != "running"
            || sample >= count
        {
            return Err(invalid("Cannot start this repetition"));
        }
        tx.execute(
            "INSERT INTO attempts(work_item,sample,state,started_at) VALUES(?,?,?,?)",
            params![work.0, sample, "running", now()],
        )?;
        let id = AttemptId(tx.last_insert_rowid());
        tx.execute("UPDATE work_items SET state='running' WHERE id=?", [work.0])?;
        tx.execute("UPDATE run_revisions SET state='running' WHERE (run,revision) IN (SELECT run,revision FROM work_items WHERE id=?) AND state='planned'",[work.0])?;
        changed(&tx)?;
        tx.commit()?;
        Ok(id)
    }

    pub fn finish_attempt<P: StoredProcedure>(
        &mut self,
        attempt: AttemptId,
        record: &AttemptRecord<P::Output>,
        directory: Option<&Path>,
    ) -> Result<()> {
        let tx = self.connection.transaction()?;
        let (contract,state,work_state,run_state):(String,String,String,String)=tx.query_row("SELECT e.contract,a.state,w.state,r.state FROM attempts a JOIN work_items w ON w.id=a.work_item JOIN runs r ON r.id=w.run JOIN experiments e ON e.id=r.experiment WHERE a.id=?",[attempt.0],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        if decode::<Contract>(&contract)? != contract_for::<P>() {
            return Err(invalid("Attempt output contract mismatch"));
        }
        if state != "running" || work_state != "running" || run_state != "running" {
            return Err(invalid("Attempt is already finished or interrupted"));
        }
        let outcome = match &record.outcome {
            crate::experiment::AttemptOutcome::Success(_) => "success",
            crate::experiment::AttemptOutcome::Failure(_) => "failure",
            crate::experiment::AttemptOutcome::Unsupported(_) => "unsupported",
        };
        tx.execute(
            "UPDATE attempts SET state='complete',record=?,outcome=?,artifact_state=? WHERE id=?",
            params![
                encode(record)?,
                outcome,
                if directory.is_some() {
                    "retained"
                } else {
                    "discarded"
                },
                attempt.0
            ],
        )?;
        if let Some(directory) = directory {
            fn files(
                root: &Path,
                dir: &Path,
                out: &mut Vec<(String, String)>,
            ) -> std::io::Result<()> {
                for entry in std::fs::read_dir(dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    let kind = entry.file_type()?;
                    if kind.is_dir() {
                        files(root, &path, out)?;
                    } else if kind.is_file() {
                        out.push((
                            path.strip_prefix(root).unwrap().to_string_lossy().into(),
                            path.to_string_lossy().into(),
                        ));
                    }
                }
                Ok(())
            }
            let mut artifacts = Vec::new();
            if let Err(error) = files(directory, directory, &mut artifacts) {
                log::warn!("Cannot list artifacts: {error}");
            }
            artifacts.sort();
            for (name, path) in artifacts {
                tx.execute(
                    "INSERT INTO artifacts(attempt,name,path) VALUES(?,?,?)",
                    params![attempt.0, name, path],
                )?;
            }
        }
        changed(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn finish_work_item(&mut self, work: WorkItemId) -> Result<()> {
        let tx = self.connection.transaction()?;
        let (planned,finished,state):(u32,u32,String)=tx.query_row("SELECT w.planned_repetitions,(SELECT count(*) FROM attempts WHERE work_item=w.id AND state='complete'),w.state FROM work_items w JOIN runs r ON r.id=w.run WHERE w.id=? AND r.state='running'",[work.0],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if planned != finished || !matches!(state.as_str(), "planned" | "running") {
            return Err(invalid(
                "Work item has unfinished repetitions or is already terminal",
            ));
        }
        tx.execute(
            "UPDATE work_items SET state='complete' WHERE id=?",
            [work.0],
        )?;
        tx.execute("UPDATE run_revisions SET state='complete' WHERE (run,revision) IN (SELECT run,revision FROM work_items WHERE id=?) AND NOT EXISTS(SELECT 1 FROM work_items w WHERE w.run=run_revisions.run AND w.revision=run_revisions.revision AND w.state NOT IN ('complete','build_failed'))",[work.0])?;
        changed(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn fail_revision(&mut self, run: RunId, revision: &str, reason: &str) -> Result<()> {
        let tx = self.connection.transaction()?;
        let plan: String = tx.query_row(
            "SELECT plan FROM runs WHERE id=? AND state='running'",
            [run.0],
            |r| r.get(0),
        )?;
        if !decode::<RunPlan>(&plan)?
            .revisions
            .iter()
            .any(|r| r.id().id() == revision)
        {
            return Err(invalid("Revision is not in the run"));
        }
        let started:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work_items WHERE run=? AND revision=? AND state!='planned')",params![run.0,revision],|r|r.get(0))?;
        if started {
            return Err(invalid("Revision already started"));
        }
        tx.execute(
            "UPDATE work_items SET state='build_failed',reason=? WHERE run=? AND revision=?",
            params![reason, run.0, revision],
        )?;
        tx.execute(
            "UPDATE run_revisions SET state='build_failed',reason=? WHERE run=? AND revision=?",
            params![reason, run.0, revision],
        )?;
        changed(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn finish_run(&mut self, run: RunId) -> Result<()> {
        let tx = self.connection.transaction()?;
        let unfinished:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work_items WHERE run=? AND state NOT IN ('complete','build_failed'))",[run.0],|r|r.get(0))?;
        if unfinished {
            return Err(invalid("Run has unfinished work"));
        }
        if tx.execute(
            "UPDATE runs SET state='complete',finished_at=? WHERE id=? AND state='running'",
            params![now(), run.0],
        )? != 1
        {
            return Err(invalid("Run is not active"));
        }
        tx.execute("UPDATE run_revisions SET state='complete' WHERE run=? AND state IN ('planned','running')",[run.0])?;
        changed(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Explicit local recovery; no timestamp-based inference or automatic retry.
    pub fn interrupt_run(&mut self, run: RunId, reason: &str) -> Result<()> {
        let tx = self.connection.transaction()?;
        if tx.execute(
            "UPDATE runs SET state='interrupted',finished_at=? WHERE id=? AND state='running'",
            params![now(), run.0],
        )? != 1
        {
            return Err(invalid("Run is not active"));
        }
        tx.execute("UPDATE attempts SET state='interrupted' WHERE state='running' AND work_item IN (SELECT id FROM work_items WHERE run=?)",[run.0])?;
        tx.execute("UPDATE work_items SET state='interrupted',reason=? WHERE run=? AND state IN ('planned','running')",params![reason,run.0])?;
        tx.execute("UPDATE run_revisions SET state='interrupted',reason=? WHERE run=? AND state IN ('planned','running')",params![reason,run.0])?;
        changed(&tx)?;
        tx.commit()?;
        Ok(())
    }
}
