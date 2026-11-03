//! Versioned analysis data. Original inputs and records remain authoritative.
use super::*;
use crate::{
    experiment::{AttemptOutcome, AttemptRecord, BenchmarkIn, BenchmarkOut, InputDescription},
    metrics::{MetricDescriptor, MetricSpec, MetricValue},
};
use rusqlite::{OptionalExtension, Statement, Transaction, TransactionBehavior, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{Hash, Hasher},
    marker::PhantomData,
};

// Bump when projection semantics, labels, descriptors or observation identities change.
const VERSION: u32 = 1;

pub(crate) type AnalysisFrame<P> = ResultFrame<
    AnalysisInput<<P as crate::experiment::Procedure>::Input>,
    NumericOutput<<P as crate::experiment::Procedure>::Output>,
>;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub(crate) struct AnalysisInput<I> {
    description: InputDescription,
    warnings: Vec<String>,
    #[serde(skip)]
    kind: PhantomData<fn() -> I>,
}
impl<I> PartialEq for AnalysisInput<I> {
    fn eq(&self, other: &Self) -> bool {
        self.description.id == other.description.id
    }
}
impl<I> Eq for AnalysisInput<I> {}
impl<I> Hash for AnalysisInput<I> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.description.id.hash(h);
    }
}
impl<I: BenchmarkIn> BenchmarkIn for AnalysisInput<I> {
    fn fields() -> Vec<crate::experiment::InputField> {
        I::fields()
    }
    fn parameters(&self) -> BTreeMap<String, serde_json::Value> {
        self.description.parameters.clone()
    }
    fn label(&self) -> String {
        self.description.label.clone()
    }
    fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(bound(serialize = "", deserialize = ""))]
pub(crate) struct NumericOutput<O> {
    values: BTreeMap<String, (Option<f64>, Option<String>)>,
    sources: Vec<(String, String)>,
    #[serde(skip)]
    identity_fallback: Option<O>,
}
#[contracts::contract_trait]
impl<O: BenchmarkOut> BenchmarkOut for NumericOutput<O> {
    fn list_metrics() -> Vec<MetricDescriptor> {
        vec![]
    }
    fn get_metric(&self, _: &str) -> Option<MetricValue> {
        None
    }
    fn get_scalar(&self, path: &str) -> Option<f64> {
        self.values.get(path)?.0
    }
    fn observation_sources(&self) -> Option<Vec<(String, String)>> {
        Some(self.sources.clone())
    }
    fn observation_identity(&self, path: &str) -> Option<String> {
        if let Some(output) = &self.identity_fallback {
            return output.observation_identity(path);
        }
        self.values.get(path).and_then(|v| v.1.clone()).or_else(|| {
            self.sources.iter().find_map(|(prefix, source)| {
                path.strip_prefix(prefix).map(|s| format!("{source}{s}"))
            })
        })
    }
}

struct Writer<'a> {
    experiment: ExperimentId,
    descriptors: Vec<MetricDescriptor>,
    known: BTreeSet<String>,
    ids: BTreeMap<String, i64>,
    metric: Statement<'a>,
    value: Statement<'a>,
    record: Statement<'a>,
}
impl<'a> Writer<'a> {
    fn new<P: StoredProcedure>(
        connection: &'a Connection,
        experiment: ExperimentId,
    ) -> Result<Self> {
        let mut statement = connection
            .prepare("SELECT path,id FROM analysis_metrics WHERE experiment=? ORDER BY position")?;
        let ids: BTreeMap<_, _> = statement
            .query_map([experiment.0], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(Self {
            experiment, descriptors: P::Output::list_metrics(), known: ids.keys().cloned().collect(), ids,
            metric: connection.prepare("INSERT INTO analysis_metrics(experiment,path,position,spec) VALUES(?,?,?,?) RETURNING id")?,
            value: connection.prepare("INSERT INTO analysis_values(attempt,metric,value,identity) VALUES(?,?,?,?)")?,
            record: connection.prepare("INSERT INTO analysis_records(attempt,summary,sources) VALUES(?,?,?)")?,
        })
    }
    fn output<O: BenchmarkOut>(
        &mut self,
        attempt: AttemptId,
        record: &AttemptRecord<O>,
        measurement: bool,
    ) -> Result<()> {
        let mut sources = Some(vec![]);
        let outcome = match &record.outcome {
            AttemptOutcome::Success(output) => {
                sources = output.observation_sources();
                if measurement {
                    let tree = output.metrics();
                    for metric in
                        crate::metrics::describe_new(&self.descriptors, &tree, &mut self.known)
                    {
                        let id = self.metric.query_row(
                            params![
                                self.experiment.0,
                                metric.path,
                                self.ids.len() as i64,
                                encode(&metric)?
                            ],
                            |r| r.get(0),
                        )?;
                        self.ids.insert(metric.path, id);
                    }
                    fn visit<O: BenchmarkOut>(
                        writer: &mut Writer<'_>,
                        attempt: AttemptId,
                        output: &O,
                        prefix: &str,
                        value: &MetricValue,
                    ) -> Result<()> {
                        match value {
                            MetricValue::Group(fields) => {
                                for (name, value) in fields {
                                    if !name.contains('.') {
                                        visit(writer, attempt, output, &join(prefix, name), value)?;
                                    }
                                }
                            }
                            MetricValue::OrderedGroup(fields) => {
                                for (name, value) in fields {
                                    if !name.contains('.') {
                                        visit(writer, attempt, output, &join(prefix, name), value)?;
                                    }
                                }
                            }
                            MetricValue::Samples(_) => (),
                            value => {
                                if let Some(id) = writer.ids.get(prefix) {
                                    writer.value.execute(params![
                                        attempt.0,
                                        id,
                                        value.as_scalar(),
                                        output.observation_identity(prefix)
                                    ])?;
                                }
                            }
                        }
                        Ok(())
                    }
                    visit(self, attempt, output, "", &tree)?;
                }
                AttemptOutcome::Success(())
            }
            AttemptOutcome::Failure(reason) => AttemptOutcome::Failure(reason.clone()),
            AttemptOutcome::Unsupported(reason) => AttemptOutcome::Unsupported(reason.clone()),
        };
        let summary = AttemptRecord {
            started_at: record.started_at,
            duration: record.duration,
            outcome,
            // Full diagnostics remain in the authoritative record used by inspection.
            diagnostics: vec![],
        };
        self.record
            .execute(params![attempt.0, encode(&summary)?, encode(&sources)?])?;
        Ok(())
    }
}
fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.into()
    } else {
        format!("{prefix}.{name}")
    }
}

impl SqliteStore {
    pub(crate) fn project_input<I: BenchmarkIn>(
        connection: &Connection,
        id: InputId,
        input: &I,
    ) -> Result<()> {
        connection.execute("INSERT INTO analysis_inputs(input,version,label,parameters,warnings) VALUES(?,?,?,?,?) ON CONFLICT(input) DO UPDATE SET version=excluded.version,label=excluded.label,parameters=excluded.parameters,warnings=excluded.warnings",
            params![id.0, VERSION, input.label(), encode(&input.parameters())?, encode(&input.warnings())?])?;
        Ok(())
    }
    pub(crate) fn ensure_projection<P: StoredProcedure>(
        &self,
        experiment: ExperimentId,
    ) -> Result<()> {
        self.validate::<P>(experiment)?;
        let version: Option<u32> = self
            .connection
            .query_row(
                "SELECT version FROM analysis_state WHERE experiment=?",
                [experiment.0],
                |r| r.get(0),
            )
            .optional()?;
        if version == Some(VERSION) {
            return Ok(());
        }
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        Self::ensure_projection_on::<P>(&tx, experiment)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn ensure_projection_on<P: StoredProcedure>(
        connection: &Connection,
        experiment: ExperimentId,
    ) -> Result<()> {
        let version: Option<u32> = connection
            .query_row(
                "SELECT version FROM analysis_state WHERE experiment=?",
                [experiment.0],
                |r| r.get(0),
            )
            .optional()?;
        if version == Some(VERSION) {
            return Ok(());
        }
        connection.execute("DELETE FROM analysis_records WHERE attempt IN (SELECT a.id FROM attempts a JOIN work_items w ON w.id=a.work_item JOIN runs r ON r.id=w.run WHERE r.experiment=?)", [experiment.0])?;
        connection.execute(
            "DELETE FROM analysis_metrics WHERE experiment=?",
            [experiment.0],
        )?;
        let mut inputs = connection.prepare("SELECT DISTINCT i.id,i.payload FROM inputs i JOIN suite_inputs s ON s.input=i.id JOIN definitions d ON d.id=s.definition WHERE d.experiment=?")?;
        for row in inputs.query_map([experiment.0], |r| {
            Ok((InputId(r.get(0)?), r.get::<_, String>(1)?))
        })? {
            let (id, payload) = row?;
            Self::project_input(connection, id, &decode::<P::Input>(&payload)?)?;
        }
        for (position, metric) in P::Output::analysis_metrics().iter().enumerate() {
            connection.execute(
                "INSERT INTO analysis_metrics(experiment,path,position,spec) VALUES(?,?,?,?)",
                params![experiment.0, metric.path, position as i64, encode(metric)?],
            )?;
        }
        let mut writer = Writer::new::<P>(connection, experiment)?;
        let mut records = connection.prepare("SELECT a.id,a.record,coalesce(json_extract(r.plan,'$.purpose'),'measurement')='measurement' FROM attempts a JOIN work_items w ON w.id=a.work_item JOIN runs r ON r.id=w.run WHERE r.experiment=? AND a.record IS NOT NULL ORDER BY a.id")?;
        for row in records.query_map([experiment.0], |r| {
            Ok((
                AttemptId(r.get(0)?),
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })? {
            let (attempt, record, measurement) = row?;
            writer.output(
                attempt,
                &decode::<AttemptRecord<P::Output>>(&record)?,
                measurement,
            )?;
        }
        connection.execute("INSERT INTO analysis_state(experiment,version) VALUES(?,?) ON CONFLICT(experiment) DO UPDATE SET version=excluded.version", params![experiment.0, VERSION])?;
        Ok(())
    }
    pub(crate) fn project_record<P: StoredProcedure>(
        connection: &Connection,
        experiment: ExperimentId,
        attempt: AttemptId,
        record: &AttemptRecord<P::Output>,
        measurement: bool,
    ) -> Result<()> {
        Writer::new::<P>(connection, experiment)?.output(attempt, record, measurement)
    }
    pub(crate) fn projected_metrics<P: StoredProcedure>(
        &self,
        experiment: ExperimentId,
    ) -> Result<Vec<MetricSpec>> {
        self.ensure_projection::<P>(experiment)?;
        let mut statement = self
            .connection
            .prepare("SELECT spec FROM analysis_metrics WHERE experiment=? ORDER BY position")?;
        statement
            .query_map([experiment.0], |r| r.get::<_, String>(0))?
            .map(|r| decode(&r?))
            .collect()
    }
    pub(crate) fn projected_inputs<P: StoredProcedure>(
        &self,
        definition: DefinitionId,
        selection: &InputSelection,
    ) -> Result<BTreeMap<InputId, AnalysisInput<P::Input>>> {
        let mut statement = self.connection.prepare("SELECT p.input,p.label,p.parameters,p.warnings FROM suite_inputs s JOIN analysis_inputs p ON p.input=s.input WHERE s.definition=? ORDER BY s.position")?;
        let mut inputs = BTreeMap::new();
        for row in statement.query_map([definition.0], |r| {
            Ok((
                InputId(r.get(0)?),
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })? {
            let (id, label, parameters, warnings) = row?;
            inputs.insert(
                id,
                AnalysisInput {
                    description: InputDescription {
                        id,
                        label,
                        parameters: decode(&parameters)?,
                    },
                    warnings: decode(&warnings)?,
                    kind: PhantomData,
                },
            );
        }
        Self::select_inputs(inputs, selection)
    }
    pub(crate) fn projected_frame<P: StoredProcedure>(
        &self,
        selection: &ResultSelection,
        paths: &[String],
    ) -> Result<AnalysisFrame<P>> {
        self.read_frame::<P, _, _>(selection,
            |definition, selected| self.projected_inputs::<P>(definition, selected),
            |works| {
                let mut attempts = Vec::new();
                let mut positions = BTreeMap::new();
                let mut headers = self.connection.prepare("SELECT a.id,a.sample,a.state,a.started_at,a.artifact_state,p.summary,p.sources,CASE WHEN p.sources='null' THEN a.record END FROM attempts a LEFT JOIN analysis_records p ON p.attempt=a.id WHERE a.work_item=? ORDER BY a.sample,a.id")?;
                let mut values = self.connection.prepare("SELECT v.attempt,v.value,v.identity FROM attempts a JOIN analysis_values v ON v.attempt=a.id WHERE a.work_item=? AND v.metric=?")?;
                let mut ids = Vec::new();
                for path in paths {
                    if let Some(id) = self.connection.query_row("SELECT id FROM analysis_metrics WHERE experiment=? AND path=?", params![selection.experiment.0, path], |r| r.get::<_, i64>(0)).optional()? {
                        ids.push((path, id));
                    }
                }
                for work in works {
                    for row in headers.query_map([work.id.0], |r| Ok((AttemptId(r.get(0)?), r.get(1)?, r.get::<_, String>(2)?, r.get(3)?, r.get::<_, String>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, Option<String>>(6)?, r.get::<_, Option<String>>(7)?)))? {
                        let (id, sample_index, state, started_at, artifact_state, summary, sources, fallback) = row?;
                        let record = summary.map(|summary| -> Result<_> {
                            let record: AttemptRecord<()> = decode(&summary)?;
                            let outcome = match record.outcome {
                                AttemptOutcome::Success(()) => AttemptOutcome::Success(NumericOutput {
                                    values: BTreeMap::new(),
                                    sources: decode::<Option<Vec<(String, String)>>>(sources.as_deref().unwrap_or("[]"))?.unwrap_or_default(),
                                    identity_fallback: fallback.as_deref().map(|raw| -> Result<_> {
                                        match decode::<AttemptRecord<P::Output>>(raw)?.outcome {
                                            AttemptOutcome::Success(output) => Ok(Some(output)),
                                            _ => Ok(None),
                                        }
                                    }).transpose()?.flatten(),
                                }),
                                AttemptOutcome::Failure(reason) => AttemptOutcome::Failure(reason),
                                AttemptOutcome::Unsupported(reason) => AttemptOutcome::Unsupported(reason),
                            };
                            Ok(AttemptRecord { started_at: record.started_at, duration: record.duration, outcome, diagnostics: record.diagnostics })
                        }).transpose()?;
                        positions.insert(id, attempts.len());
                        attempts.push(StoredAttempt { id, work_item: work.id, sample_index, state: super::read::state(state)?, artifact_state: decode(&format!("\"{artifact_state}\""))?, started_at, record });
                    }
                    for (path, id) in &ids {
                        for row in values.query_map(params![work.id.0, id], |r| Ok((AttemptId(r.get(0)?), r.get::<_, Option<f64>>(1)?, r.get::<_, Option<String>>(2)?)))? {
                            let (attempt, value, identity) = row?;
                            if let Some(AttemptRecord { outcome: AttemptOutcome::Success(output), .. }) = &mut attempts[positions[&attempt]].record {
                                output.values.insert((*path).clone(), (value, identity));
                            }
                        }
                    }
                }
                Ok(attempts)
            },
        )
    }
}
