use std::{io, path::PathBuf};

use yosys_bench::src_manager::{
    build::CcacheCMakeBuilder,
    git::{GitManager, history::CommitId},
};

const YOSYS_REPOSITORY: &str = "https://github.com/YosysHQ/yosys.git";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let revisions = vec![
        CommitId::new("a31d95ea005b35bb894876e27627030bf8ef2c8a".into()),
        CommitId::new("8802950c22d4dd32abd482f409c83396d04bd496".into()),
    ];
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("output");
    let source_dir = output.join("yosys_source");
    let ccache_dir = output.join("ccache");

    let mut git = GitManager::new(&source_dir, "main", Some(revisions[0].clone()))
        .with_remote(YOSYS_REPOSITORY);
    git.initialize()?;
    let builder = CcacheCMakeBuilder::new(&ccache_dir)
        .configure_arg("-DYOSYS_COMPILER_LAUNCHER=ccache")
        .configure_arg("-DYOSYS_USE_BUNDLED_LIBS=ON")
        .configure_arg("-DYOSYS_ENABLE_UNIT_TESTS=OFF");

    builder.set_max_cache_size("20GB")?;
    builder.reset_stats()?;

    for revision in &revisions {
        if git.active_commit() != Some(revision) {
            git.set_revision(revision)?;
        }
        let build_dir = output.join("yosys_build").join(revision.id());
        let install_dir = output.join("yosys_install").join(revision.id());
        let prefix = builder.build(&source_dir, &build_dir, &install_dir)?;

        let yosys = prefix.join("bin/yosys");
        if !yosys.is_file() {
            return Err(
                io::Error::other(format!("missing installed Yosys: {}", yosys.display())).into(),
            );
        }
        println!("Installed Yosys {} to {}", revision.id(), prefix.display());
        let stats = builder.cache_stats()?;
        println!("ccache: {} hits, {} misses", stats.hits(), stats.misses());
    }
    Ok(())
}
