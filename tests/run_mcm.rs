use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
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
fn printed_configuration_has_no_side_effects() {
    let root = tempfile::tempdir().unwrap();
    let output = run(Command::new(env!("CARGO_BIN_EXE_run_mcm"))
        .current_dir(root.path())
        .arg("--print-config"));
    let config: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        config["revisions"]["Explicit"].as_array().unwrap().len(),
        35
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn cli_builds_two_revisions_and_runs_composed_mcm_without_storage() {
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
    let config = root.path().join("config.json");
    fs::write(
        &config,
        serde_json::to_vec(&serde_json::json!({
            "repository": repository, "branch": "main", "revisions": {"Explicit": [first, second]},
            "output_dir": root.path().join("output"), "paper": paper, "library": library,
            "coefficient_sets": [[3, -11]],
            "artifacts": {"logs":false, "scripts":false, "netlists":false}
        }))
        .unwrap(),
    )
    .unwrap();
    let output = run(Command::new(env!("CARGO_BIN_EXE_run_mcm"))
        .current_dir(root.path())
        .arg(&config));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let attempts = report["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["revision"]["id"], first);
    assert_eq!(attempts[1]["revision"]["id"], second);
    assert!(
        attempts
            .iter()
            .all(|attempt| attempt["outcome"]["Success"].is_object())
    );
    assert_eq!(
        attempts[0]["outcome"]["Success"]["paper"]["id"],
        attempts[1]["outcome"]["Success"]["paper"]["id"]
    );
    assert_eq!(
        fs::read_to_string(root.path().join("paper.calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let directory = Path::new(report["directory"].as_str().unwrap());
    assert!(directory.join("results.json").is_file());
    assert_eq!(fs::read_dir(directory).unwrap().count(), 1);
    assert!(git(&repository, &["status", "--porcelain"]).is_empty());
    assert_eq!(git(&repository, &["rev-parse", "HEAD"]), second);
}
