use std::{
    ffi::OsString,
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use super::BuildError;

/// Builds CMake projects with one compiler cache shared across revisions.
#[derive(Debug, Clone)]
pub struct CcacheCMakeBuilder {
    ccache_dir: PathBuf,
    jobs: Option<NonZeroUsize>,
    build_type: String,
    configure_args: Vec<OsString>,
}

impl CcacheCMakeBuilder {
    /// Reuse this cache directory for each revision's build.
    pub fn new(ccache_dir: impl Into<PathBuf>) -> Self {
        Self {
            ccache_dir: ccache_dir.into(),
            jobs: std::thread::available_parallelism().ok(),
            build_type: "RelWithDebInfo".into(),
            configure_args: Vec::new(),
        }
    }

    pub fn jobs(mut self, jobs: NonZeroUsize) -> Self {
        self.jobs = Some(jobs);
        self
    }

    pub fn configure_arg(mut self, arg: impl Into<OsString>) -> Self {
        self.configure_args.push(arg.into());
        self
    }

    pub fn build_type(mut self, build_type: impl Into<String>) -> Self {
        self.build_type = build_type.into();
        self
    }

    /// Use a separate `build_dir` and `install_dir` for each revision.
    pub fn build(
        &self,
        source_dir: &Path,
        build_dir: &Path,
        install_dir: &Path,
    ) -> Result<PathBuf, BuildError> {
        let source_dir = source_dir.canonicalize()?;
        if !source_dir.join("CMakeLists.txt").is_file() {
            return Err(BuildError::MissingCMakeProject(source_dir));
        }
        fs::create_dir_all(build_dir)?;
        fs::create_dir_all(install_dir)?;
        fs::create_dir_all(&self.ccache_dir)?;
        let build_dir = build_dir.canonicalize()?;
        let install_dir = install_dir.canonicalize()?;
        let ccache_dir = self.ccache_dir.canonicalize()?;

        run(
            "ccache check",
            Command::new("ccache")
                .arg("--version")
                .stdout(Stdio::null()),
        )?;

        let mut prefix = OsString::from("-DCMAKE_INSTALL_PREFIX=");
        prefix.push(&install_dir);
        run(
            "configure",
            Command::new("cmake")
                .arg("-S")
                .arg(&source_dir)
                .arg("-B")
                .arg(&build_dir)
                .arg(format!("-DCMAKE_BUILD_TYPE={}", self.build_type))
                .args(&self.configure_args)
                .arg("-DCMAKE_C_COMPILER_LAUNCHER=ccache")
                .arg("-DCMAKE_CXX_COMPILER_LAUNCHER=ccache")
                .arg(prefix)
                .env("CCACHE_DIR", &ccache_dir),
        )?;

        let mut build = Command::new("cmake");
        build
            .arg("--build")
            .arg(&build_dir)
            .arg("--config")
            .arg(&self.build_type);
        if let Some(jobs) = self.jobs {
            build.arg("--parallel").arg(jobs.get().to_string());
        }
        run("build", build.env("CCACHE_DIR", &ccache_dir))?;
        run(
            "install",
            Command::new("cmake")
                .arg("--install")
                .arg(&build_dir)
                .arg("--config")
                .arg(&self.build_type)
                .env("CCACHE_DIR", &ccache_dir),
        )?;
        Ok(install_dir)
    }
}

fn run(stage: &'static str, command: &mut Command) -> Result<(), BuildError> {
    let status = command
        .status()
        .map_err(|source| BuildError::CommandIo { stage, source })?;
    if !status.success() {
        return Err(BuildError::Command { stage, status });
    }
    Ok(())
}
