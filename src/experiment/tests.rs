use std::{cell::Cell, io, time::Duration};

use serde::{Deserialize, Serialize};

use super::*;
use crate::{
    benchmarks::synth::{
        CellLibrary, TimingConfig, TimingInput, TimingOutput, YosysBenchmark, YosysProcedure,
        design::{Design, SourceFile},
    },
    tools::yosys::{Yosys, YosysRun},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct Input(Vec<i64>);

impl BenchmarkIn for Input {}

struct Tools {
    calls: Cell<u32>,
}

struct TestProcedure;

impl Procedure for TestProcedure {
    type Input = Input;
    type Tools = Tools;
    type Output = TimingOutput;

    fn name(&self) -> &str {
        "test"
    }

    fn run(
        &self,
        _input: &Input,
        tools: &Tools,
        context: &mut RunContext,
    ) -> Result<TimingOutput, ExperimentError> {
        let call = tools.calls.get() + 1;
        tools.calls.set(call);
        if call == 2 {
            return Err(io::Error::other("intermittent failure").into());
        }
        if call == 3 {
            context.record_diagnostic("perf", Err(ExperimentError::Profiling("no stacks".into())));
        }
        Ok(TimingOutput::new(Duration::from_millis(call.into()), ()))
    }
}

struct GeneratedSuite {
    coefficient: Cell<i64>,
    procedure: TestProcedure,
}

impl Suite for GeneratedSuite {
    type Tool = ();
    type Session = ();
    type Procedure = TestProcedure;

    fn name(&self) -> &str {
        "generated"
    }

    fn procedure(&self) -> &TestProcedure {
        &self.procedure
    }

    fn inputs(&self) -> Result<Vec<Input>, ExperimentError> {
        Ok(vec![Input(vec![self.coefficient.get()])])
    }

    fn begin(&self) -> Result<(), ExperimentError> {
        Ok(())
    }

    fn tools(&self, _tool: &(), _session: &()) -> Result<Tools, ExperimentError> {
        Ok(Tools {
            calls: Cell::new(0),
        })
    }
}

fn definition(values: &[i64]) -> SuiteDefinition<Input> {
    SuiteDefinition::new(
        ProcedureId::new("test", Version::default()),
        values.iter().map(|value| Input(vec![*value])).collect(),
    )
    .unwrap()
}

#[test]
fn membership_controls_suite_version_without_changing_execution_order() {
    let version = Version::new(2, 3);
    let previous = SuiteDefinition::new(
        ProcedureId::new("test", version),
        definition(&[1, 2]).inputs().clone(),
    )
    .unwrap();
    for (values, relation, expected_version) in [
        (vec![2, 1], CmpSuite::Eq, version),
        (vec![3, 1, 2], CmpSuite::Superset, Version::new(2, 4)),
        (vec![1], CmpSuite::Incompatible, Version::new(3, 0)),
        (vec![1, 3], CmpSuite::Incompatible, Version::new(3, 0)),
    ] {
        let current = definition(&values);
        assert_eq!(current.compare(&previous), relation);
        assert_eq!(current.next_version(&previous), expected_version);
        let current = current.with_previous(&previous);
        assert_eq!(current.version(), &expected_version);
        assert_eq!(current.procedure().version(), &expected_version);
        assert_eq!(
            current.inputs(),
            &values
                .into_iter()
                .map(|value| Input(vec![value]))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn procedure_name_changes_are_incompatible_even_with_identical_inputs() {
    let previous = definition(&[1, 2]);
    let changed = SuiteDefinition::new(
        ProcedureId::new("another-procedure", Version::default()),
        previous.inputs().clone(),
    )
    .unwrap();
    assert_eq!(changed.compare(&previous), CmpSuite::Incompatible);
    assert_eq!(
        changed.with_previous(&previous).version(),
        &Version::new(2, 0)
    );
}

#[test]
fn resolved_versions_do_not_feed_back_into_membership_comparison() {
    let previous = definition(&[1, 2]);
    let expanded = definition(&[1, 2, 3]).with_previous(&previous);
    assert_eq!(expanded.version(), &Version::new(1, 1));
    assert_eq!(expanded.compare(&previous), CmpSuite::Superset);
    let repeated = definition(&[3, 2, 1]).with_previous(&expanded);
    assert_eq!(repeated.version(), expanded.version());
    assert_eq!(repeated.compare(&expanded), CmpSuite::Eq);
    let decoded: SuiteDefinition<Input> =
        serde_json::from_str(&serde_json::to_string(&repeated).unwrap()).unwrap();
    assert_eq!(decoded.procedure(), repeated.procedure());
    assert_eq!(decoded.version(), &Version::new(1, 1));
}

#[test]
fn duplicate_inputs_are_rejected_in_construction_and_deserialization() {
    assert!(matches!(
        SuiteDefinition::new(
            ProcedureId::new("test", Version::default()),
            vec![Input(vec![1]); 2]
        ),
        Err(ExperimentError::DuplicateInput { index: 1 })
    ));
    let mut json = serde_json::to_value(definition(&[1])).unwrap();
    json["inputs"] = serde_json::json!([[1], [1]]);
    assert!(serde_json::from_value::<SuiteDefinition<Input>>(json).is_err());
}

#[test]
fn each_invocation_uses_current_generation_and_records_actual_inputs() {
    let suite = GeneratedSuite {
        coefficient: Cell::new(7),
        procedure: TestProcedure,
    };
    let first = suite.definition().unwrap();
    suite.coefficient.set(11);
    let second = suite.definition().unwrap();

    assert_eq!(first.inputs(), &[Input(vec![7])]);
    assert_eq!(second.inputs(), &[Input(vec![11])]);
    assert_eq!(second.compare(&first), CmpSuite::Incompatible);
    let recorded: SuiteDefinition<Input> =
        serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap();
    assert_eq!(recorded.compare(&first), CmpSuite::Eq);
    assert_eq!(recorded.inputs(), &[Input(vec![7])]);
}

#[test]
fn equal_inputs_can_have_different_outputs_and_outcomes() {
    let suite = GeneratedSuite {
        coefficient: Cell::new(7),
        procedure: TestProcedure,
    };
    let input = Input(vec![7]);
    let tools = suite.tools(&(), &()).unwrap();
    let mut context = RunContext::new("unused".into(), ArtifactPolicy::default());
    let first = suite.procedure().run(&input, &tools, &mut context).unwrap();
    assert!(suite.procedure().run(&input, &tools, &mut context).is_err());
    let third = suite.procedure().run(&input, &tools, &mut context).unwrap();

    assert_ne!(first.runtime(), third.runtime());
    assert_eq!(suite.definition().unwrap().inputs(), &[input]);
}

struct InspectYosys;

impl YosysProcedure for InspectYosys {
    type Config = TimingConfig;
    type Measurements = ();

    fn name(&self) -> &str {
        "inspect-yosys"
    }

    fn job(&self, input: &TimingInput) -> Result<crate::tools::yosys::YosysJob, ExperimentError> {
        assert_eq!(input.design().top.as_deref(), Some("top"));
        assert_eq!(input.config().custom_commands(), &["stat"]);
        Ok(crate::tools::yosys::YosysJob {
            design: input.design().clone(),
            ..crate::tools::yosys::YosysJob::default()
        })
    }

    fn parse(&self, _input: &TimingInput, _output: &YosysRun) -> Result<(), ExperimentError> {
        Ok(())
    }
}

#[test]
fn yosys_adapter_keeps_tools_out_of_workload_identity() {
    let input = TimingInput::new(
        Design {
            files: vec![SourceFile::new("top.v", "module top; endmodule")],
            top: Some("top".into()),
            ..Design::default()
        },
        TimingConfig::new(vec!["stat".into()]),
    );
    let procedure = YosysBenchmark::new(InspectYosys);
    let temporary = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(
        temporary.path().join("workspace"),
        ArtifactPolicy::new(false, false, false),
    );
    for name in ["revision-a", "revision-b"] {
        use std::os::unix::fs::PermissionsExt;
        let path = temporary.path().join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        procedure
            .run(&input, &Yosys::new(path), &mut context)
            .unwrap();
    }
    let recorded: TimingInput =
        serde_json::from_str(&serde_json::to_string(&input).unwrap()).unwrap();
    assert_eq!(recorded, input);
    assert_eq!(procedure.name(), "inspect-yosys");
}

#[test]
fn file_contents_and_logical_paths_determine_identity_independently_of_root() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    std::fs::write(first.join("design.v"), "original contents").unwrap();
    std::fs::write(second.join("design.v"), "original contents").unwrap();

    let source = SourceFile::from_path(&first, "design.v").unwrap();
    let library = CellLibrary::from_path(first.join("design.v")).unwrap();
    assert_eq!(source, SourceFile::from_path(&second, "design.v").unwrap());
    assert_eq!(
        library,
        CellLibrary::from_path(second.join("design.v")).unwrap()
    );

    std::fs::write(first.join("design.v"), "changed contents").unwrap();
    assert_ne!(source, SourceFile::from_path(&first, "design.v").unwrap());
    assert_ne!(
        library,
        CellLibrary::from_path(first.join("design.v")).unwrap()
    );
}

struct InvocationProcedure;

impl Procedure for InvocationProcedure {
    type Input = Input;
    type Tools = std::rc::Rc<Cell<u32>>;
    type Output = TimingOutput;

    fn name(&self) -> &str {
        "invocation"
    }
    fn run(
        &self,
        input: &Input,
        tools: &Self::Tools,
        context: &mut RunContext,
    ) -> Result<TimingOutput, ExperimentError> {
        tools.set(tools.get() + 1);
        std::fs::write(
            context.workspace().join("attempt.txt"),
            input.0[0].to_string(),
        )?;
        match input.0[0] {
            2 => Err(ExperimentError::Config("failed workload".into())),
            3 => Err(ExperimentError::Unsupported("unsupported workload".into())),
            _ => Ok(TimingOutput::new(
                Duration::from_millis(tools.get().into()),
                (),
            )),
        }
    }
}

struct InvocationSuite {
    generated: Cell<i64>,
    sessions: Cell<u32>,
    procedure: InvocationProcedure,
}

impl Suite for InvocationSuite {
    type Tool = ();
    type Session = std::rc::Rc<Cell<u32>>;
    type Procedure = InvocationProcedure;
    fn name(&self) -> &str {
        "invocation"
    }
    fn procedure(&self) -> &InvocationProcedure {
        &self.procedure
    }
    fn inputs(&self) -> Result<Vec<Input>, ExperimentError> {
        Ok((self.generated.get()..self.generated.get() + 3)
            .map(|v| Input(vec![v]))
            .collect())
    }
    fn begin(&self) -> Result<Self::Session, ExperimentError> {
        self.sessions.set(self.sessions.get() + 1);
        Ok(std::rc::Rc::new(Cell::new(0)))
    }
    fn tools(&self, _tool: &(), session: &Self::Session) -> Result<Self::Session, ExperimentError> {
        Ok(std::rc::Rc::clone(session))
    }
}

struct Provider {
    builds: usize,
}

impl ToolProvider for Provider {
    type Tool = ();
    fn revisions(
        &mut self,
        _selection: &crate::src_manager::git::history::RevisionSelection,
    ) -> Result<Vec<crate::src_manager::git::history::Commit>, ExperimentError> {
        Ok(["1", "2"]
            .into_iter()
            .map(|id| {
                serde_json::from_value(serde_json::json!({
                    "id": id.repeat(40), "committed_at": std::time::UNIX_EPOCH,
                    "author": "test", "commit_message": "test revision",
                }))
                .unwrap()
            })
            .collect())
    }
    fn tool(
        &mut self,
        _revision: &crate::src_manager::git::history::Commit,
    ) -> Result<(), ExperimentError> {
        self.builds += 1;
        Ok(())
    }
}

#[test]
fn experiment_keeps_attempt_order_continues_failures_and_refreshes_session_and_inputs() {
    let root = tempfile::tempdir().unwrap();
    let suite = InvocationSuite {
        generated: Cell::new(1),
        sessions: Cell::new(0),
        procedure: InvocationProcedure,
    };
    let experiment = Experiment::new(
        crate::src_manager::git::history::RevisionSelection::All,
        suite,
        root.path().into(),
    );
    let mut provider = Provider { builds: 0 };
    let report = experiment
        .run(
            &mut provider,
            &RunOptions {
                artifacts: ArtifactPolicy::new(false, false, false),
                perf: None,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(provider.builds, 2);
    assert_eq!(report.attempts.len(), 6);
    assert_eq!(experiment.suite().sessions.get(), 1);
    for offset in [0, 3] {
        assert!(
            matches!(&report.attempts[offset].record.outcome, AttemptOutcome::Success(out) if *out.runtime() == Duration::from_millis((offset + 1) as u64))
        );
        assert!(
            matches!(&report.attempts[offset + 1].record.outcome, AttemptOutcome::Failure(reason) if reason.contains("failed workload"))
        );
        assert!(matches!(
            &report.attempts[offset + 2].record.outcome,
            AttemptOutcome::Unsupported(_)
        ));
        assert_eq!(report.attempts[offset + 1].input_index, 1);
    }
    assert_eq!(std::fs::read_dir(&report.directory).unwrap().count(), 0);
    assert!(
        report
            .attempts
            .iter()
            .all(|attempt| attempt.artifacts.is_none())
    );
    assert_ne!(
        report.attempts[0].revision.id(),
        report.attempts[3].revision.id()
    );
    experiment.suite().generated.set(7);
    let next = experiment
        .run_with_previous(
            &mut provider,
            &RunOptions::default(),
            Some(&report.definition),
        )
        .unwrap();
    assert_eq!(report.definition.version(), &Version::new(1, 0));
    assert_eq!(next.definition.procedure().version(), &Version::new(2, 0));
    assert_eq!(experiment.suite().sessions.get(), 2);
    assert_eq!(next.definition.inputs()[0], Input(vec![7]));
    assert_eq!(
        next.definition.compare(&report.definition),
        CmpSuite::Incompatible
    );
    assert_ne!(report.directory, next.directory);
    assert!(
        matches!(&next.attempts[0].record.outcome, AttemptOutcome::Success(out) if *out.runtime() == Duration::from_millis(1))
    );
    assert!(next.attempts.iter().all(|attempt| {
        attempt
            .artifacts
            .as_ref()
            .unwrap()
            .join("attempt.txt")
            .is_file()
    }));
}

#[test]
fn shared_sampling_retains_mixed_outcomes_and_does_not_change_identity() {
    let root = tempfile::tempdir().unwrap();
    let experiment = Experiment::new(
        crate::src_manager::git::history::RevisionSelection::All,
        GeneratedSuite {
            coefficient: Cell::new(7),
            procedure: TestProcedure,
        },
        root.path().into(),
    );
    let mut provider = Provider { builds: 0 };
    let mut options = RunOptions {
        repetitions: NonZeroU32::new(3).unwrap(),
        artifacts: ArtifactPolicy::new(false, false, false),
        ..RunOptions::default()
    };
    let report = experiment.run(&mut provider, &options).unwrap();
    assert_eq!(report.attempts.len(), 6);
    for samples in report.attempts.chunks_exact(3) {
        assert!(matches!(
            samples[0].record.outcome,
            AttemptOutcome::Success(_)
        ));
        assert!(matches!(
            samples[1].record.outcome,
            AttemptOutcome::Failure(_)
        ));
        assert!(matches!(
            samples[2].record.outcome,
            AttemptOutcome::Success(_)
        ));
        assert_eq!(
            samples
                .iter()
                .map(|sample| sample.sample_index)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(samples[2].record.diagnostics.len(), 1);
        assert!(samples[2].record.diagnostics[0].contains("no stacks"));
    }
    options.repetitions = NonZeroU32::new(1).unwrap();
    let next = experiment.run(&mut provider, &options).unwrap();
    assert_eq!(report.definition.compare(&next.definition), CmpSuite::Eq);
}

#[test]
fn build_failures_retain_previous_results_and_allow_later_revisions() {
    struct FailingProvider {
        calls: u32,
    }
    impl ToolProvider for FailingProvider {
        type Tool = ();
        fn revisions(
            &mut self,
            selection: &crate::src_manager::git::history::RevisionSelection,
        ) -> Result<Vec<crate::src_manager::git::history::Commit>, ExperimentError> {
            let mut revisions = Provider { builds: 0 }.revisions(selection)?;
            revisions.push(
                serde_json::from_value(serde_json::json!({
                    "id": "3".repeat(40), "committed_at": std::time::UNIX_EPOCH,
                    "author": "test", "commit_message": "third revision",
                }))
                .unwrap(),
            );
            Ok(revisions)
        }
        fn tool(
            &mut self,
            _revision: &crate::src_manager::git::history::Commit,
        ) -> Result<(), ExperimentError> {
            self.calls += 1;
            if self.calls == 2 {
                Err(io::Error::other("deliberate build failure").into())
            } else {
                Ok(())
            }
        }
    }
    let root = tempfile::tempdir().unwrap();
    let experiment = Experiment::new(
        crate::src_manager::git::history::RevisionSelection::All,
        GeneratedSuite {
            coefficient: Cell::new(7),
            procedure: TestProcedure,
        },
        root.path().into(),
    );
    let report = experiment
        .run(&mut FailingProvider { calls: 0 }, &RunOptions::default())
        .unwrap();
    assert_eq!(report.attempts.len(), 2);
    assert!(
        report
            .attempts
            .iter()
            .all(|attempt| matches!(attempt.record.outcome, AttemptOutcome::Success(_)))
    );
    assert_eq!(report.revision_failures.len(), 1);
    assert!(
        report.revision_failures[0]
            .reason
            .contains("deliberate build failure")
    );
    assert_eq!(
        report.revision_failures[0].revision.id().id(),
        &"2".repeat(40)
    );
    assert_eq!(report.attempts[1].revision.id().id(), &"3".repeat(40));
    let serialized = serde_json::to_value(&report).unwrap();
    assert_eq!(serialized["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(serialized["revision_failures"].as_array().unwrap().len(), 1);
}
