use std::{io, path::PathBuf, process::ExitCode};
use yosys_bencher::progress::Progress;

use yosys_bencher::src_manager::{
    build::CcacheCMakeBuilder,
    git::{GitManager, history::CommitId},
};

const YOSYS_REPOSITORY: &str = "https://github.com/YosysHQ/yosys.git";

fn main() -> ExitCode {
    let progress = Progress::new("Yosys");
    match run(&progress) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            progress.fail("Build run failed");
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(progress: &Progress) -> Result<(), Box<dyn std::error::Error>> {
    let revisions = [
        CommitId::new("a31d95ea005b35bb894876e27627030bf8ef2c8a".into()),
        CommitId::new("8802950c22d4dd32abd482f409c83396d04bd496".into()),
    ];
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("output");
    let source_dir = output.join("yosys_source");
    let ccache_dir = output.join("ccache");

    let mut git = GitManager::new(&source_dir, "main", Some(revisions[0].clone()))
        .with_remote(YOSYS_REPOSITORY);
    progress.stage("Preparing repository");
    git.initialize()?;
    let builder = CcacheCMakeBuilder::new(&ccache_dir)
        .configure_arg("-DYOSYS_COMPILER_LAUNCHER=ccache")
        .configure_arg("-DYOSYS_USE_BUNDLED_LIBS=ON")
        .configure_arg("-DYOSYS_ENABLE_UNIT_TESTS=OFF")
        .with_progress(progress.clone());

    builder.set_max_cache_size("20GB")?;
    builder.reset_stats()?;

    progress.start(revisions.len() as u64);
    let mut failures = 0;
    for (index, revision) in revisions.iter().enumerate() {
        let short: String = revision.id().chars().take(10).collect();
        progress.context(format!("rev {}/{} {short}", index + 1, revisions.len()));
        let result = (|| -> Result<PathBuf, Box<dyn std::error::Error>> {
            progress.stage("Checking out revision");
            if git.active_commit() != Some(revision) {
                git.set_revision(revision)?;
            }
            let build_dir = output.join("yosys_build").join(revision.id());
            let install_dir = output.join("yosys_install").join(revision.id());
            let prefix = builder.build(&source_dir, &build_dir, &install_dir)?;
            if !prefix.join("bin/yosys").is_file() {
                return Err(io::Error::other(format!(
                    "missing installed Yosys: {}",
                    prefix.display()
                ))
                .into());
            }
            Ok(prefix)
        })();
        match result {
            Ok(prefix) => {
                progress.println(format!("Installed {short}: {}", prefix.display()));
                match builder.cache_stats() {
                    Ok(stats) => progress.println(format!(
                        "ccache: {} hits, {} misses",
                        stats.hits(),
                        stats.misses()
                    )),
                    Err(error) => progress.println(format!("ccache stats unavailable: {error}")),
                }
            }
            Err(error) => {
                failures += 1;
                progress.println(format!("Skipping {short}: {error}"));
            }
        }
        progress.advance(1);
    }
    progress.finish(format!(
        "Done: {} installed, {failures} failed",
        revisions.len() - failures
    ));
    if failures > 0 {
        return Err(format!("{failures} revisions failed; see the logs listed above").into());
    }
    Ok(())
}
