use std::{fs, path::Path};
use yosys_bench::{
    benchmarks::{
        mcm::{CoefficientSets, McmSuite, PaperTool},
        synth::CellLibrary,
    },
    experiment::{ArtifactPolicy, Experiment, RunOptions, YosysProvider},
    src_manager::{
        build::CcacheCMakeBuilder,
        git::{
            GitManager,
            history::{CommitId, RevisionSelection},
        },
    },
    tools::perf::PerfStat,
};

mod support;

#[test]
fn experiment_builds_two_revisions_and_runs_composed_mcm_without_storage() {
    let root = tempfile::tempdir().unwrap();
    let repository = support::Repository::new(root.path().join("original"));
    support::executable(
        &repository.path().join("yosys"),
        include_str!("fixtures/yosys.sh"),
    );
    let first = repository.commit(&[("CMakeLists.txt", "cmake_minimum_required(VERSION 3.16)\nproject(fake_yosys NONE)\ninstall(PROGRAMS yosys DESTINATION bin)\n")]);
    let second = repository.commit(&[("revision.txt", "second")]);
    let paper = root.path().join("paper");
    support::executable(&paper, include_str!("fixtures/paper.sh"));
    let output_dir = root.path().join("output");
    let perf = root.path().join("perf");
    support::executable(&perf, include_str!("fixtures/perf.sh"));
    let suite = McmSuite::new(
        CoefficientSets::Explicit(vec![vec![3, -11]]),
        CellLibrary::new("library(test) {}\n"),
        PaperTool { executable: paper },
    );
    let mut provider = YosysProvider {
        git: GitManager::new(output_dir.join("source"), "main", None)
            .with_remote(repository.path().as_os_str()),
        builder: CcacheCMakeBuilder::new(output_dir.join("ccache")).with_suppressed_stdout(),
        output_dir: output_dir.join("builds"),
    };
    let experiment = Experiment::new(
        RevisionSelection::Explicit(vec![
            CommitId::new(first.clone()),
            CommitId::new(second.clone()),
        ]),
        suite,
        output_dir,
    );
    let report = experiment
        .run(
            &mut provider,
            &RunOptions {
                artifacts: ArtifactPolicy::new(false, false, false),
                perf: None,
                perf_counters: Some(PerfStat::new(&perf)),
                repetitions: std::num::NonZeroU32::new(2).unwrap(),
            },
        )
        .unwrap();
    let report = serde_json::to_value(report).unwrap();
    let attempts = report["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 4);
    assert_eq!(attempts[0]["revision"]["id"], first);
    assert_eq!(attempts[2]["revision"]["id"], second);
    assert!(
        attempts
            .iter()
            .all(|attempt| attempt["outcome"]["Success"].is_object())
    );
    for attempt in attempts {
        for phase in ["baseline", "optimized", "reference"] {
            let counters = &attempt["outcome"]["Success"][phase]["perf"]["counters"];
            assert_eq!(counters[1]["value"], 100.5);
            assert_eq!(counters[1]["running_percent"], 50.0);
            assert!(counters[3]["value"].is_null());
        }
    }
    assert_eq!(
        fs::read_to_string(perf.with_extension("calls"))
            .unwrap()
            .lines()
            .count(),
        11
    );
    assert_eq!(
        attempts[0]["outcome"]["Success"]["reference"]["id"],
        attempts[2]["outcome"]["Success"]["reference"]["id"]
    );
    assert_eq!(
        attempts[1]["outcome"]["Success"]["reference"]["id"],
        attempts[3]["outcome"]["Success"]["reference"]["id"]
    );
    assert_ne!(
        attempts[0]["outcome"]["Success"]["reference"]["id"],
        attempts[1]["outcome"]["Success"]["reference"]["id"]
    );
    assert_eq!(
        fs::read_to_string(root.path().join("paper.calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    let directory = Path::new(report["directory"].as_str().unwrap());
    assert_eq!(fs::read_dir(directory).unwrap().count(), 0);
    assert!(repository.git(&["status", "--porcelain"]).is_empty());
    assert_eq!(repository.git(&["rev-parse", "HEAD"]), second);
}
