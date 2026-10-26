use std::{cell::Cell, fs, io, rc::Rc, time::Duration};

use serde::{Deserialize, Serialize};

use super::*;
use crate::{
    benchmarks::synth::TimingOutput,
    src_manager::git::history::{Commit, RevisionSelection},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct Input(i64);
impl BenchmarkIn for Input {}

fn definition(name: &str, values: &[i64], version: Version) -> SuiteDefinition<Input> {
    SuiteDefinition::new(
        ProcedureId::new(name, version),
        values.iter().copied().map(Input).collect(),
    )
    .unwrap()
}

#[test]
fn suite_identity_and_version_follow_membership_while_preserving_execution_order() {
    let previous = definition("test", &[1, 2], Version::new(2, 3));
    for (name, values, relation, (major, minor)) in [
        ("test", vec![2, 1], CmpSuite::Eq, (2, 3)),
        ("test", vec![3, 1, 2], CmpSuite::Superset, (2, 4)),
        ("test", vec![1], CmpSuite::Incompatible, (3, 0)),
        ("test", vec![1, 3], CmpSuite::Incompatible, (3, 0)),
        ("renamed", vec![1, 2], CmpSuite::Incompatible, (3, 0)),
    ] {
        let current = definition(name, &values, Version::default());
        assert_eq!(current.compare(&previous), relation);
        let resolved = current.with_previous(&previous);
        assert_eq!(resolved.version(), &Version::new(major, minor));
        assert_eq!(resolved.compare(&previous), relation);
        assert_eq!(
            resolved.inputs(),
            &values.into_iter().map(Input).collect::<Vec<_>>()
        );
        let repeated = resolved.clone().with_previous(&resolved);
        assert_eq!(repeated.compare(&resolved), CmpSuite::Eq);
        assert_eq!(repeated.version(), resolved.version());
    }
}

#[test]
fn suite_construction_and_deserialization_enforce_unique_inputs() {
    let original = definition("test", &[1, 2], Version::new(2, 3));
    let json = serde_json::to_string(&original).unwrap();
    let restored: SuiteDefinition<Input> = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.procedure(), original.procedure());
    assert_eq!(restored.inputs(), original.inputs());
    assert!(matches!(
        SuiteDefinition::new(original.procedure().clone(), vec![Input(1); 2]),
        Err(ExperimentError::DuplicateInput { index: 1 })
    ));
    let mut json = serde_json::to_value(&original).unwrap();
    json["inputs"] = serde_json::json!([1, 1]);
    assert!(serde_json::from_value::<SuiteDefinition<Input>>(json).is_err());
}

struct TestProcedure;
impl Procedure for TestProcedure {
    type Input = Input;
    type Tools = Rc<Cell<u32>>;
    type Output = TimingOutput;

    fn name(&self) -> &str {
        "test"
    }
    fn run(
        &self,
        input: &Input,
        calls: &Self::Tools,
        context: &mut RunContext,
    ) -> Result<TimingOutput, ExperimentError> {
        calls.set(calls.get() + 1);
        fs::write(context.workspace().join("attempt.txt"), input.0.to_string())?;
        if input.0 == 3 {
            return Err(ExperimentError::Unsupported("unsupported".into()));
        }
        if input.0 == 2 || *context.sample_index() == 1 {
            return Err(ExperimentError::Config("failed".into()));
        }
        if *context.sample_index() == 2 {
            context.record_diagnostic("perf", Err(ExperimentError::Profiling("no stacks".into())));
        }
        Ok(TimingOutput::new(
            Duration::from_millis(calls.get().into()),
            (),
        ))
    }
}

#[derive(Default)]
struct TestSuite {
    first: Cell<i64>,
    sessions: Cell<u32>,
}
impl Suite for TestSuite {
    type Tool = ();
    type Session = Rc<Cell<u32>>;
    type Procedure = TestProcedure;

    fn name(&self) -> &str {
        "test"
    }
    fn procedure(&self) -> &TestProcedure {
        &TestProcedure
    }
    fn inputs(&self, _: &Self::Session) -> Result<Vec<Input>, ExperimentError> {
        Ok((self.first.get()..self.first.get() + 3)
            .map(Input)
            .collect())
    }
    fn begin(&self) -> Result<Self::Session, ExperimentError> {
        self.sessions.set(self.sessions.get() + 1);
        Ok(Rc::new(Cell::new(0)))
    }
    fn tools(&self, _: &(), session: &Self::Session) -> Result<Self::Session, ExperimentError> {
        Ok(Rc::clone(session))
    }
}

#[derive(Default)]
struct Provider {
    builds: usize,
    fail: Option<usize>,
}
impl ToolProvider for Provider {
    type Tool = ();
    fn revisions(&mut self, _: &RevisionSelection) -> Result<Vec<Commit>, ExperimentError> {
        Ok((1..=3)
            .map(|id| {
                serde_json::from_value(serde_json::json!({
                    "id": id.to_string().repeat(40), "committed_at": std::time::UNIX_EPOCH,
                    "author": "test", "commit_message": "test revision",
                }))
                .unwrap()
            })
            .collect())
    }
    fn tool(&mut self, _: &Commit) -> Result<(), ExperimentError> {
        self.builds += 1;
        if self.fail == Some(self.builds) {
            return Err(io::Error::other("build failed").into());
        }
        Ok(())
    }
}

fn options() -> RunOptions {
    RunOptions {
        artifacts: ArtifactPolicy::new(false, false, false),
        perf_counters: None,
        ..RunOptions::default()
    }
}

#[test]
fn execution_preserves_revision_input_sample_order_and_all_outcomes() {
    let root = tempfile::tempdir().unwrap();
    let suite = TestSuite {
        first: Cell::new(1),
        ..Default::default()
    };
    let experiment = Experiment::new(RevisionSelection::All, suite, root.path().into());
    let mut provider = Provider {
        fail: Some(2),
        ..Default::default()
    };
    let options = RunOptions {
        repetitions: NonZeroU32::new(3).unwrap(),
        ..options()
    };
    let report = experiment.run(&mut provider, &options).unwrap();
    assert_eq!(provider.builds, 3);
    assert_eq!(report.revision_failures.len(), 1);
    assert_eq!(
        report.revision_failures[0].revision.id().id(),
        &"2".repeat(40)
    );
    assert!(report.revision_failures[0].reason.contains("build failed"));
    assert_eq!(report.attempts.len(), 18);
    for (index, attempt) in report.attempts.iter().enumerate() {
        let (input, sample) = (index / 3 % 3, index % 3);
        let revision = if index < 9 { "1" } else { "3" };
        assert_eq!(attempt.revision.id().id(), &revision.repeat(40));
        assert_eq!(
            (attempt.input_index, attempt.sample_index),
            (input, sample as u32)
        );
        match &attempt.record.outcome {
            AttemptOutcome::Unsupported(reason) => {
                assert_eq!(input, 2);
                assert_eq!(reason, "unsupported");
            }
            AttemptOutcome::Failure(reason) => {
                assert!(input == 1 || (input == 0 && sample == 1));
                assert!(reason.contains("failed"));
            }
            AttemptOutcome::Success(output) => {
                assert_eq!(input, 0);
                assert_ne!(sample, 1);
                assert_eq!(*output.runtime(), Duration::from_millis(index as u64 + 1));
            }
        }
        assert_eq!(
            attempt.record.diagnostics.len(),
            usize::from(input == 0 && sample == 2)
        );
        assert!(attempt.artifacts.is_none());
    }
    assert_eq!(fs::read_dir(&report.directory).unwrap().count(), 0);
}

#[test]
fn invocations_refresh_inputs_and_session_and_retain_requested_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let experiment = Experiment::new(
        RevisionSelection::All,
        TestSuite::default(),
        root.path().into(),
    );
    let mut provider = Provider::default();
    let first = experiment.run(&mut provider, &options()).unwrap();
    experiment.suite().first.set(7);
    let retained = RunOptions {
        artifacts: ArtifactPolicy::default(),
        ..options()
    };
    let next = experiment
        .run_with_previous(&mut provider, &retained, Some(&first.definition))
        .unwrap();
    assert_eq!(experiment.suite().sessions.get(), 2);
    assert_eq!(first.definition.inputs(), &[Input(0), Input(1), Input(2)]);
    assert_eq!(next.definition.inputs(), &[Input(7), Input(8), Input(9)]);
    assert_eq!(next.definition.version(), &Version::new(2, 0));
    assert_ne!(first.directory, next.directory);
    assert!(
        matches!(&next.attempts[0].record.outcome, AttemptOutcome::Success(output) if *output.runtime() == Duration::from_millis(1))
    );
    for attempt in &next.attempts {
        let path = attempt.artifacts.as_ref().unwrap().join("attempt.txt");
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            next.definition.inputs()[attempt.input_index].0.to_string()
        );
    }
}
