use std::{
    ffi::OsString,
    fs::{self, File},
    io::Write,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use super::BuildError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Getters)]
pub struct CcacheStats {
    #[get = "pub"]
    direct_hits: u64,
    #[get = "pub"]
    preprocessed_hits: u64,
    #[get = "pub"]
    misses: u64,
}

impl CcacheStats {
    pub fn new(direct_hits: u64, preprocessed_hits: u64, misses: u64) -> Self {
        Self {
            direct_hits,
            preprocessed_hits,
            misses,
        }
    }

    pub fn hits(self) -> u64 {
        self.direct_hits + self.preprocessed_hits
    }
}

#[derive(Debug, Clone, Getters)]
pub struct CcacheCMakeBuilder {
    #[get]
    ccache_dir: PathBuf,
    #[get]
    jobs: Option<NonZeroUsize>,
    #[get]
    build_type: String,
    #[get]
    configure_args: Vec<OsString>,
    #[get]
    #[get = "pub"]
    progress: crate::progress::Progress,
}

impl CcacheCMakeBuilder {
    pub fn measurement_options(&self) -> serde_json::Value {
        serde_json::json!({ "build_type": self.build_type,
            "configure_args": self.configure_args.iter().map(|v| v.to_string_lossy()).collect::<Vec<_>>() })
    }

    pub fn new(ccache_dir: impl Into<PathBuf>) -> Self {
        Self {
            ccache_dir: ccache_dir.into(),
            jobs: std::thread::available_parallelism().ok(),
            build_type: "RelWithDebInfo".into(),
            configure_args: Vec::new(),
            progress: crate::progress::Progress::default(),
        }
    }

    pub fn with_jobs(mut self, jobs: NonZeroUsize) -> Self {
        self.jobs = Some(jobs);
        self
    }

    pub fn configure_arg(mut self, arg: impl Into<OsString>) -> Self {
        self.configure_args.push(arg.into());
        self
    }

    pub fn with_build_type(mut self, build_type: impl Into<String>) -> Self {
        self.build_type = build_type.into();
        self
    }

    /// Build stdout and stderr are always logged; retained for existing callers.
    pub fn with_suppressed_stdout(self) -> Self {
        self
    }

    pub fn with_progress(mut self, progress: crate::progress::Progress) -> Self {
        self.progress = progress;
        self
    }

    pub fn set_progress(&mut self, progress: crate::progress::Progress) {
        self.progress = progress;
    }

    pub fn set_max_cache_size(&self, size: &str) -> Result<(), BuildError> {
        fs::create_dir_all(&self.ccache_dir)?;
        self.run(
            "ccache size",
            Command::new("ccache")
                .arg("--max-size")
                .arg(size)
                .env("CCACHE_DIR", &self.ccache_dir),
            None,
        )
    }

    /// Zero statistics without clearing cached compiler results.
    pub fn reset_stats(&self) -> Result<(), BuildError> {
        fs::create_dir_all(&self.ccache_dir)?;
        self.run(
            "ccache reset",
            Command::new("ccache")
                .arg("--zero-stats")
                .env("CCACHE_DIR", &self.ccache_dir),
            None,
        )
    }

    pub fn cache_stats(&self) -> Result<CcacheStats, BuildError> {
        let stage = "ccache stats";
        let output = Command::new("ccache")
            .arg("--print-stats")
            .env("CCACHE_DIR", &self.ccache_dir)
            .output()
            .map_err(|source| BuildError::CommandIo { stage, source })?;
        if !output.status.success() {
            let (mut file, log) = self.command_log(stage)?;
            file.write_all(&output.stdout)?;
            file.write_all(&output.stderr)?;
            return Err(BuildError::Command {
                stage,
                status: output.status,
                log,
            });
        }
        CcacheStats::parse(&output.stdout)
    }

    pub fn build(
        &self,
        source_dir: &Path,
        build_dir: &Path,
        install_dir: &Path,
    ) -> Result<PathBuf, BuildError> {
        self.build_with_logs(source_dir, build_dir, install_dir, build_dir)
    }

    /// Keep logs separately so callers can prune build and install directories.
    pub fn build_with_logs(
        &self,
        source_dir: &Path,
        build_dir: &Path,
        install_dir: &Path,
        logs_dir: &Path,
    ) -> Result<PathBuf, BuildError> {
        let source_dir = source_dir.canonicalize()?;
        if !source_dir.join("CMakeLists.txt").is_file() {
            return Err(BuildError::MissingCMakeProject(source_dir));
        }
        fs::create_dir_all(build_dir)?;
        fs::create_dir_all(install_dir)?;
        fs::create_dir_all(&self.ccache_dir)?;
        fs::create_dir_all(logs_dir)?;
        let build_dir = build_dir.canonicalize()?;
        let install_dir = install_dir.canonicalize()?;
        let ccache_dir = self.ccache_dir.canonicalize()?;
        let logs = tempfile::Builder::new()
            .prefix("logs-")
            .tempdir_in(logs_dir.canonicalize()?)?
            .keep();
        let base_dir = source_dir
            .ancestors()
            .find(|ancestor| build_dir.starts_with(ancestor))
            .unwrap_or(&source_dir);
        let debug_prefix_map = format!("-fdebug-prefix-map={}=.", build_dir.display());
        let compiler_flags = |name| {
            let mut flags = std::env::var_os(name).unwrap_or_default();
            if !flags.is_empty() {
                flags.push(" ");
            }
            flags.push(&debug_prefix_map);
            flags
        };

        self.run(
            "ccache check",
            Command::new("ccache").arg("--version"),
            None,
        )?;

        let mut prefix = OsString::from("-DCMAKE_INSTALL_PREFIX=");
        prefix.push(&install_dir);
        self.run(
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
                .env("CCACHE_DIR", &ccache_dir)
                .env("CCACHE_BASEDIR", base_dir)
                .env("CFLAGS", compiler_flags("CFLAGS"))
                .env("CXXFLAGS", compiler_flags("CXXFLAGS")),
            Some(&logs.join("configure.log")),
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
        self.run(
            "build",
            build
                .env("CCACHE_DIR", &ccache_dir)
                .env("CCACHE_BASEDIR", base_dir),
            Some(&logs.join("build.log")),
        )?;
        self.run(
            "install",
            Command::new("cmake")
                .arg("--install")
                .arg(&build_dir)
                .arg("--config")
                .arg(&self.build_type)
                .env("CCACHE_DIR", &ccache_dir)
                .env("CCACHE_BASEDIR", base_dir),
            Some(&logs.join("install.log")),
        )?;
        Ok(install_dir)
    }

    fn command_log(&self, stage: &str) -> Result<(File, PathBuf), BuildError> {
        fs::create_dir_all(&self.ccache_dir)?;
        let file = tempfile::Builder::new()
            .prefix(&format!("{}-", stage.replace(' ', "-")))
            .suffix(".log")
            .tempfile_in(&self.ccache_dir)?;
        let (file, path) = file.keep().map_err(|error| error.error)?;
        Ok((file, path.canonicalize()?))
    }

    fn run(
        &self,
        stage: &'static str,
        command: &mut Command,
        log: Option<&Path>,
    ) -> Result<(), BuildError> {
        match stage {
            "configure" => self.progress.stage("Configuring Yosys"),
            "build" => self.progress.stage("Building Yosys"),
            "install" => self.progress.stage("Installing Yosys"),
            _ => self.progress.detail(stage),
        }
        let (file, log) = match log {
            Some(path) => (File::create(path)?, path.to_path_buf()),
            None => self.command_log(stage)?,
        };
        command
            .stdin(Stdio::null())
            .stdout(file.try_clone()?)
            .stderr(file);
        let status = command
            .status()
            .map_err(|source| BuildError::CommandIo { stage, source })?;
        if !status.success() {
            return Err(BuildError::Command { stage, status, log });
        }
        Ok(())
    }
}

impl CcacheStats {
    fn parse(output: &[u8]) -> Result<Self, BuildError> {
        let text = std::str::from_utf8(output)
            .map_err(|error| BuildError::InvalidCacheStats(error.to_string()))?;
        let mut direct_hits = None;
        let mut preprocessed_hits = None;
        let mut misses = None;
        for line in text.lines() {
            let Some((key, value)) = line.split_once('\t') else {
                continue;
            };
            let field = match key {
                "direct_cache_hit" => &mut direct_hits,
                "preprocessed_cache_hit" => &mut preprocessed_hits,
                "cache_miss" => &mut misses,
                _ => continue,
            };
            *field =
                Some(value.parse().map_err(|_| {
                    BuildError::InvalidCacheStats(format!("invalid {key}: {value}"))
                })?);
        }
        Ok(Self {
            direct_hits: direct_hits
                .ok_or_else(|| BuildError::InvalidCacheStats("missing direct_cache_hit".into()))?,
            preprocessed_hits: preprocessed_hits.ok_or_else(|| {
                BuildError::InvalidCacheStats("missing preprocessed_cache_hit".into())
            })?,
            misses: misses
                .ok_or_else(|| BuildError::InvalidCacheStats("missing cache_miss".into()))?,
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn build_output_is_logged_and_failure_keeps_the_log() {
        let temp = tempfile::tempdir().unwrap();
        let builder = CcacheCMakeBuilder::new(temp.path());
        let log = temp.path().join("build.log");
        let error = builder
            .run(
                "build",
                Command::new("sh").args([
                    "-c",
                    "printf 'compiler output\n'; printf 'compiler error\n' >&2; exit 7",
                ]),
                Some(&log),
            )
            .unwrap_err();
        let BuildError::Command {
            status,
            log: recorded,
            ..
        } = error
        else {
            panic!("expected command failure")
        };
        assert_eq!(status.code(), Some(7));
        assert_eq!(recorded, log);
        let contents = fs::read_to_string(log).unwrap();
        assert!(contents.contains("compiler output"));
        assert!(contents.contains("compiler error"));
    }

    #[test]
    fn successful_command_keeps_both_streams_without_terminal_output() {
        let temp = tempfile::tempdir().unwrap();
        let builder = CcacheCMakeBuilder::new(temp.path());
        let log = temp.path().join("configure.log");
        builder
            .run(
                "configure",
                Command::new("sh").args(["-c", "printf 'configured\n'; printf 'warning\n' >&2"]),
                Some(&log),
            )
            .unwrap();
        let contents = fs::read_to_string(log).unwrap();
        assert!(contents.contains("configured"));
        assert!(contents.contains("warning"));
    }
}
