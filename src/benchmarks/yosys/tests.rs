use super::*;
use crate::{
    analysis::{AnalysisPlan, Backend, Paging},
    benchmarks::synth::{TimingConfig, TimingProcedure},
    experiment::{
        ArtifactPolicy, AttemptOutcome, AttemptRecord, ProcedureId, ProcessCapture,
        ProcessExecutor, SuiteDefinition, Version,
    },
    metrics::{Direction, scalar},
    store::{ExperimentSpec, ResultSelection, RevisionQuery, RunPlan, SqliteStore},
    tools::perf::PreparedPerfStat,
};
use serde_json::json;
use std::{
    fs,
    path::Path,
    process::{Command, ExitStatus, Output},
    time::SystemTime,
};

type Timing = YosysBenchmark<TimingProcedure>;

const REPORT: &[u8] = br#"{
    "generator": "Yosys test",
    "total_ns": 5000000,
    "passes": {
        "opt": {"runtime_ns": 3000000, "num_calls": 4},
        "plugin.pass%": {"runtime_ns": 2000000, "num_calls": 1}
    }
}"#;

#[derive(Debug)]
struct ReportExecutor;
impl ProcessExecutor for ReportExecutor {
    fn execute(
        &self,
        command: &mut Command,
        _: &Path,
        _: Option<&PreparedPerfStat>,
    ) -> Result<ProcessCapture> {
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.windows(2)
                .any(|a| a == ["--perffile", ".yosys-logs/timing.json"])
        );
        assert!(!args.iter().any(|a| a == "-T"));
        let directory = command.get_current_dir().unwrap();
        let script = fs::read_to_string(directory.join("run.ys"))?;
        for line in script.lines().filter(|line| line.starts_with("tee ")) {
            fs::write(
                directory.join(line.split_whitespace().nth(2).unwrap()),
                b"stage log",
            )?;
        }
        fs::write(directory.join(".yosys-logs/timing.json"), REPORT)?;
        Ok(ProcessCapture {
            command: format!("{command:?}"),
            output: Output {
                status: ExitStatus::default(),
                stdout: vec![],
                stderr: vec![],
            },
            elapsed: Duration::from_millis(20),
            perf: None,
            diagnostics: vec![],
        })
    }
}
fn input() -> YosysBenchmarkInput<TimingConfig> {
    YosysBenchmarkInput::new(
        design::Design {
            files: vec![design::SourceFile::new(
                "test.v",
                "module test(input a, output y); assign y = a; endmodule",
            )],
            top: Some("test".into()),
            ..Default::default()
        },
        TimingConfig::new(vec!["proc".into(), "opt".into()]),
    )
}

#[test]
fn generic_yosys_measurement_retains_internal_statistics_and_the_raw_report() {
    let directory = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(directory.path().into(), ArtifactPolicy::default())
        .with_executor(ReportExecutor);
    let procedure = Timing::new(TimingProcedure);
    let output = procedure
        .run(
            &input(),
            &Yosys::new(std::env::current_exe().unwrap()),
            &mut context,
        )
        .unwrap();
    assert_eq!(*output.runtime(), Duration::from_millis(20));
    let timings = output.timings().as_ref().unwrap();
    assert_eq!(timings.total, Duration::from_millis(5));
    assert_eq!(timings.passes["opt"].runtime, Duration::from_millis(3));
    assert_eq!(timings.passes["opt"].calls, 4);
    assert_eq!(timings.generator, "Yosys test");
    assert_eq!(
        fs::read(directory.path().join("logs/yosys/timing.json")).unwrap(),
        REPORT
    );
    assert!(context.diagnostics().is_empty());
    assert_eq!(scalar(&output, "timings.passes.opt.runtime"), Some(0.003));
    assert_eq!(scalar(&output, "timings.passes.opt.calls"), Some(4.));
    assert_eq!(
        scalar(&output, "timings.passes.plugin%2Epass%25.runtime"),
        Some(0.002)
    );
    let json = serde_json::to_string(&output).unwrap();
    assert_eq!(
        serde_json::from_str::<YosysBenchmarkOutput>(&json).unwrap(),
        output
    );
}

#[test]
fn timing_reports_reject_missing_or_invalid_statistics_and_old_outputs_still_load() {
    for invalid in [
        br#"{"passes":{}}"#.as_slice(),
        br#"{"generator":"test","total_ns":1,"passes":{"opt":{"runtime_ns":-1,"num_calls":1}}}"#,
    ] {
        assert!(YosysTimings::parse(invalid).is_err());
    }
    let old: YosysBenchmarkOutput =
        serde_json::from_value(json!({"runtime":{"secs":1,"nanos":0},"measurements":null}))
            .unwrap();
    assert!(old.timings().is_none());
    assert_eq!(scalar(&old, "runtime"), Some(1.));
    assert_eq!(scalar(&old, "timings.passes.opt.runtime"), None);
}

#[test]
fn stored_pass_metrics_are_discovered_and_missing_timings_remain_gaps() {
    let mut store = SqliteStore::in_memory().unwrap();
    let definition = SuiteDefinition::new(
        ProcedureId::new("yosys.timing", Version::default()),
        vec![input()],
    )
    .unwrap();
    let revisions = ["old", "new"].map(|id| {
        serde_json::from_value(json!({
            "id":id,"committed_at":SystemTime::UNIX_EPOCH,"author":"test","commit_message":id
        }))
        .unwrap()
    });
    let run = store
        .register_run::<Timing>(
            &ExperimentSpec {
                key: "pass-timings".into(),
                name: "Pass timings".into(),
                description: String::new(),
                repository: "test".into(),
            },
            &definition,
            &RunPlan::new(revisions.into(), 1),
        )
        .unwrap();
    for revision in ["old", "new"] {
        let work = run.work_items[revision][&run.inputs[0]];
        let attempt = store.start_attempt(work, 0).unwrap();
        let mut output = YosysBenchmarkOutput::new(Duration::from_secs(1), ());
        if revision == "new" {
            output = output.with_timings(YosysTimings::parse(REPORT).unwrap());
        }
        store
            .finish_attempt::<Timing>(
                attempt,
                &AttemptRecord {
                    started_at: SystemTime::UNIX_EPOCH,
                    duration: Duration::from_secs(1),
                    outcome: AttemptOutcome::Success(output),
                    diagnostics: vec![],
                },
                None,
            )
            .unwrap();
        store.finish_work_item(work).unwrap();
    }
    store.finish_run(run.run).unwrap();
    let backend = Backend::new(store).unwrap();
    let metadata = backend.describe(run.experiment).unwrap();
    let path = "timings.passes.opt.runtime";
    let metric = metadata.metrics.iter().find(|m| m.path == path).unwrap();
    assert_eq!(metric.unit, "s");
    assert_eq!(metric.direction, Direction::Lower);
    assert!(
        metadata
            .metrics
            .iter()
            .any(|m| m.path == "timings.passes.opt.calls")
    );
    let mut plan = AnalysisPlan::history(ResultSelection::new(run.experiment), path);
    let result = backend.analyze(&plan).unwrap();
    assert_eq!(
        result.series[0]
            .points
            .iter()
            .map(|p| p.value)
            .collect::<Vec<_>>(),
        [None, Some(0.003)]
    );
    let inspection = backend
        .inspect(
            &result.snapshot,
            &result.series[0].points[1].id,
            Paging::default(),
        )
        .unwrap();
    assert_eq!(inspection.observations.items[0].value, Some(0.003));
    // Discovery spans the experiment, even when the selected revision has no such pass.
    plan.selection.revisions = RevisionQuery::Only {
        ids: vec!["old".into()],
    };
    assert!(
        backend.analyze(&plan).unwrap().series[0].points[0]
            .value
            .is_none()
    );
    plan.selection.revisions = RevisionQuery::Only {
        ids: vec!["new".into()],
    };
    plan.metric = Some("timings.passes.opt.calls".into());
    assert_eq!(
        backend.analyze(&plan).unwrap().series[0].points[0].value,
        Some(4.)
    );
}
