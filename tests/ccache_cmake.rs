use std::{
    error::Error,
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::tempdir;
use yosys_bench::src_manager::{
    build::CcacheCMakeBuilder,
    git::{GitManager, history::CommitId},
};

#[test]
fn unchanged_source_hits_ccache_across_commits() -> Result<(), Box<dyn Error>> {
    let temp = tempdir()?;
    let repository = temp.path().join("repository");
    let source = temp.path().join("source");
    let cache = temp.path().join("ccache");
    fs::create_dir(&repository)?;
    fs::write(
        repository.join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.16)\nproject(cache_test C)\nadd_executable(dummy stable.c changing.c)\ninstall(TARGETS dummy RUNTIME DESTINATION bin)\n",
    )?;
    fs::write(
        repository.join("stable.c"),
        "int stable(void) { return 1; }\n",
    )?;
    fs::write(
        repository.join("changing.c"),
        "int stable(void);\nint main(void) { return stable() - 1; }\n",
    )?;

    run(Command::new("git")
        .args(["init", "-b", "main"])
        .arg(&repository))?;
    git(&repository, &["add", "."])?;
    git(
        &repository,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "first",
        ],
    )?;
    let first = CommitId(
        String::from_utf8(git(&repository, &["rev-parse", "HEAD"])?.stdout)?
            .trim()
            .into(),
    );

    fs::write(
        repository.join("changing.c"),
        "int stable(void);\nint main(void) { return stable() + 1; }\n",
    )?;
    git(&repository, &["add", "changing.c"])?;
    git(
        &repository,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "second",
        ],
    )?;
    let second = CommitId(
        String::from_utf8(git(&repository, &["rev-parse", "HEAD"])?.stdout)?
            .trim()
            .into(),
    );

    let mut manager =
        GitManager::new(&source, "main", Some(first.clone())).with_remote(&repository);
    assert_eq!(manager.initialize()?, first);
    let builder = CcacheCMakeBuilder::new(&cache).with_suppressed_stdout();
    builder.reset_stats()?;

    let first_install = builder.build(
        &source,
        &temp.path().join("build-first"),
        &temp.path().join("install-first"),
    )?;
    assert!(first_install.join("bin/dummy").is_file());
    run(&mut Command::new(first_install.join("bin/dummy")))?;
    let first_stats = builder.cache_stats()?;
    assert_eq!(
        first_stats.misses, 2,
        "first build should compile both files"
    );
    assert_eq!(first_stats.hits(), 0);

    assert_eq!(manager.set_revision(&second)?, second);
    let second_install = builder.build(
        &source,
        &temp.path().join("build-second"),
        &temp.path().join("install-second"),
    )?;
    assert!(second_install.join("bin/dummy").is_file());
    assert_eq!(
        Command::new(second_install.join("bin/dummy"))
            .status()?
            .code(),
        Some(2)
    );
    let second_stats = builder.cache_stats()?;
    assert_eq!(
        second_stats.hits() - first_stats.hits(),
        1,
        "unchanged file should hit ccache"
    );
    assert_eq!(
        second_stats.misses - first_stats.misses,
        1,
        "changed file should miss ccache"
    );
    Ok(())
}

fn git(repo: &Path, args: &[&str]) -> Result<Output, Box<dyn Error>> {
    run(Command::new("git").arg("-C").arg(repo).args(args))
}

fn run(command: &mut Command) -> Result<Output, Box<dyn Error>> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "{command:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(output)
}
