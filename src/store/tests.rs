use std::time::{Duration, SystemTime};

use crate::{
    benchmarks::synth::{
        TimingConfig, TimingInput, TimingOutput,
        design::{Design, SourceFile},
    },
    experiment::{AttemptOutcome, BenchmarkIn, BenchmarkOut},
};

use super::*;

#[derive(Debug, Clone)]
pub struct FlatExpResults {
    results: HashMap<ExperimentId, String>,
}

#[test]
fn storage_preserves_varying_outputs_failures_and_revision_boundaries() {
    let input = TimingInput::new(
        Design {
            files: vec![SourceFile::new("top.v", "module top; endmodule")],
            top: Some("top".into()),
            ..Design::default()
        },
        TimingConfig::new(vec!["stat".into()]),
    );
    let mut first = BenchmarkResults::new(input.clone());
    for outcome in [
        AttemptOutcome::Success(TimingOutput::new(Duration::from_millis(1), ())),
        AttemptOutcome::Failure("intermittent failure".into()),
        AttemptOutcome::Success(TimingOutput::new(Duration::from_millis(3), ())),
        AttemptOutcome::Unsupported("missing pass".into()),
    ] {
        first.record(AttemptRecord {
            started_at: SystemTime::UNIX_EPOCH,
            duration: Duration::from_secs(1),
            outcome,
            diagnostics: vec!["profiling unavailable".into()],
        });
    }
    let mut second = BenchmarkResults::new(input.clone());
    second.record(AttemptRecord {
        started_at: SystemTime::UNIX_EPOCH,
        duration: Duration::from_secs(1),
        outcome: AttemptOutcome::Success(TimingOutput::new(Duration::from_millis(9), ())),
        diagnostics: Vec::new(),
    });
    let version = Version::new(1, 4);
    let procedure = ProcedureId::new("synth.timing", version);
    let first_commit = CommitId::new("first".into());
    let second_commit = CommitId::new("second".into());
    let results = ExperimentResults::new(HashMap::from([
        (
            first_commit.clone(),
            SuiteResults::new(procedure.clone(), vec![first]),
        ),
        (
            second_commit.clone(),
            SuiteResults::new(procedure.clone(), vec![second]),
        ),
    ]));
    let id = ExperimentId::new("test");
    let store = FlatExpResults {
        results: HashMap::from([(id.clone(), serde_json::to_string(&results).unwrap())]),
    };

    let first: SuiteResults<TimingInput, TimingOutput> =
        store.get_results(id.clone(), first_commit).unwrap();
    assert_eq!(first.procedure(), &procedure);
    assert_eq!(first.version(), &version);
    assert_eq!(first.benchmarks()[0].input(), &input);
    let attempts = first.benchmarks()[0].attempts();
    assert_eq!(attempts.len(), 4);
    assert_eq!(attempts[0].started_at, SystemTime::UNIX_EPOCH);
    assert_eq!(attempts[0].diagnostics, ["profiling unavailable"]);
    assert!(
        matches!(&attempts[3].outcome, AttemptOutcome::Unsupported(reason) if reason == "missing pass")
    );
    assert_eq!(attempts[0].duration, Duration::from_secs(1));
    assert!(
        matches!(&attempts[0].outcome, AttemptOutcome::Success(out) if *out.runtime() == Duration::from_millis(1))
    );
    assert!(
        matches!(&attempts[1].outcome, AttemptOutcome::Failure(reason) if reason == "intermittent failure")
    );
    assert!(
        matches!(&attempts[2].outcome, AttemptOutcome::Success(out) if *out.runtime() == Duration::from_millis(3))
    );

    let second: SuiteResults<TimingInput, TimingOutput> =
        store.get_results(id, second_commit).unwrap();
    assert_eq!(second.benchmarks()[0].attempts().len(), 1);
    assert!(
        matches!(&second.benchmarks()[0].attempts()[0].outcome, AttemptOutcome::Success(out) if *out.runtime() == Duration::from_millis(9))
    );
}

impl Store for FlatExpResults {
    fn get_results<BMI, BMO>(
        &self,
        exp_id: ExperimentId,
        commit_id: CommitId,
    ) -> Result<SuiteResults<BMI, BMO>, StoreError>
    where
        BMI: BenchmarkIn,
        BMO: BenchmarkOut,
    {
        let result: &String = self
            .results
            .get(&exp_id)
            .ok_or(StoreError::ExperimentIdNotFound(exp_id))?;
        let exp_res: ExperimentResults<BMI, BMO> = serde_json::from_str(result)?;
        let suite_res: SuiteResults<BMI, BMO> = exp_res
            .suites
            .get(&commit_id)
            .ok_or(StoreError::CommitIdNotFound(commit_id))?
            .clone();
        Ok(suite_res)
    }
}
