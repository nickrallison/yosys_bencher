use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};
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
};

fn run(command: &mut Command) -> Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn git(repository: &Path, args: &[&str]) -> String {
    String::from_utf8(run(Command::new("git").arg("-C").arg(repository).args(args)).stdout)
        .unwrap()
        .trim()
        .into()
}

fn commit(repository: &Path) -> String {
    git(repository, &["add", "."]);
    git(
        repository,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "fixture",
        ],
    );
    git(repository, &["rev-parse", "HEAD"])
}

#[test]
fn experiment_builds_two_revisions_and_runs_composed_mcm_without_storage() {
    let root = tempfile::tempdir().unwrap();
    let repository = root.path().join("original");
    fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "-b", "main"]);
    fs::write(repository.join("CMakeLists.txt"), "cmake_minimum_required(VERSION 3.16)\nproject(fake_yosys NONE)\ninstall(PROGRAMS yosys DESTINATION bin)\n").unwrap();
    fs::write(repository.join("yosys"), include_str!("fixtures/yosys.sh")).unwrap();
    fs::set_permissions(repository.join("yosys"), fs::Permissions::from_mode(0o755)).unwrap();
    let first = commit(&repository);
    fs::write(repository.join("revision.txt"), "second").unwrap();
    let second = commit(&repository);
    let paper = root.path().join("paper");
    fs::write(&paper, include_str!("fixtures/paper.sh")).unwrap();
    fs::set_permissions(&paper, fs::Permissions::from_mode(0o755)).unwrap();
    let library = root.path().join("cells.lib");
    fs::write(&library, "library(test) {}\n").unwrap();
    let output_dir = root.path().join("output");
    let suite = McmSuite::new(
        CoefficientSets::Explicit(vec![vec![3, -11]]),
        CellLibrary::from_path(library).unwrap(),
        PaperTool { executable: paper },
    );
    let mut provider = YosysProvider {
        git: GitManager::new(output_dir.join("source"), "main", None)
            .with_remote(repository.as_os_str()),
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
    assert!(git(&repository, &["status", "--porcelain"]).is_empty());
    assert_eq!(git(&repository, &["rev-parse", "HEAD"]), second);
}
