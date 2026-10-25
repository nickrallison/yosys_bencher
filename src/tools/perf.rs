use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::Serialize;

use crate::experiment::ExperimentError;

/// Tools for diagnostic recordings, kept separate from benchmark timing samples.
#[derive(Debug, Clone, Serialize, Getters)]
pub struct PerfProfiler {
    #[get]
    perf: PathBuf,
    #[get]
    perl: PathBuf,
    #[get]
    addr2line: PathBuf,
    #[get]
    stackcollapse: PathBuf,
    #[get]
    flamegraph: PathBuf,
}

impl PerfProfiler {
    /// Resolve and check prerequisites before building or starting an experiment.
    pub fn new(addr2line: impl AsRef<Path>) -> Result<Self, ExperimentError> {
        if !cfg!(target_os = "linux") {
            return Err(ExperimentError::Profiling(
                "perf recording requires Linux".into(),
            ));
        }
        let tools = Self {
            perf: resolve(Path::new("perf"))?,
            perl: resolve(Path::new("perl"))?,
            addr2line: resolve(addr2line.as_ref())?,
            stackcollapse: resolve(Path::new("stackcollapse-perf.pl"))?,
            flamegraph: resolve(Path::new("flamegraph.pl"))?,
        };
        for command in [
            Command::new(&tools.perf).arg("--version"),
            Command::new(&tools.addr2line).arg("--version"),
            Command::new(&tools.perl)
                .arg("-c")
                .arg(&tools.stackcollapse),
            Command::new(&tools.perl).arg("-c").arg(&tools.flamegraph),
        ] {
            let output = command.output().map_err(|error| {
                ExperimentError::Profiling(format!("cannot run {command:?}: {error}"))
            })?;
            if !output.status.success() {
                return Err(ExperimentError::Profiling(format!(
                    "prerequisite {command:?} failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
        }
        Ok(tools)
    }

    /// Record one extra invocation, then render source-annotated stacks to SVG.
    /// All pipeline stages must succeed; raw data and diagnostics survive failure.
    pub fn record(&self, command: &Command, directory: &Path) -> Result<(), ExperimentError> {
        fs::create_dir_all(directory)?;
        let directory = directory.canonicalize()?;
        let data = directory.join("perf.data");
        let scratch = tempfile::tempdir_in(&directory)?;
        let script = scratch.path().join("perf.script");
        let folded = scratch.path().join("stacks.folded");
        let mut record = Command::new(&self.perf);
        record
            .args(["record", "-q", "-e", "cpu-clock:u", "-F", "999"])
            .args(["--call-graph", "dwarf,16384", "-o"])
            .arg(&data)
            .arg("--")
            .arg(command.get_program())
            .args(command.get_args())
            .current_dir(command.get_current_dir().unwrap_or(&directory))
            .stdin(Stdio::null());
        for (key, value) in command.get_envs() {
            match value {
                Some(value) => {
                    record.env(key, value);
                }
                None => {
                    record.env_remove(key);
                }
            }
        }
        stage(
            &mut record,
            &directory,
            "record",
            &directory.join("yosys.stdout.log"),
        )?;
        stage(
            Command::new(&self.perf)
                .args(["script", "-i"])
                .arg(&data)
                .arg(format!("--addr2line={}", self.addr2line.display()))
                .args(["--no-inline", "-F", "+srcline"])
                .env("DEBUGINFOD_URLS", ""),
            &directory,
            "script",
            &script,
        )?;
        stage(
            Command::new(&self.perl)
                .arg(&self.stackcollapse)
                .arg("--srcline")
                .stdin(File::open(&script)?),
            &directory,
            "collapse",
            &folded,
        )?;
        if fs::metadata(&folded)?.len() == 0 {
            return Err(ExperimentError::Profiling(format!(
                "no sampled stacks in {}; the process may be too short to sample",
                data.display()
            )));
        }
        stage(
            Command::new(&self.perl)
                .arg(&self.flamegraph)
                .stdin(File::open(&folded)?),
            &directory,
            "flamegraph",
            &directory.join("flamegraph.svg"),
        )
    }
}

fn stage(
    command: &mut Command,
    directory: &Path,
    name: &str,
    stdout: &Path,
) -> Result<(), ExperimentError> {
    writeln!(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("commands.log"))?,
        "{name}: {command:?}"
    )?;
    command.stdout(File::create(stdout)?);
    let output = command
        .output()
        .map_err(|error| ExperimentError::Profiling(format!("cannot run {command:?}: {error}")))?;
    fs::write(directory.join(format!("{name}.stderr.log")), &output.stderr)?;
    if !output.status.success() {
        let tail = &output.stderr[output.stderr.len().saturating_sub(8192)..];
        return Err(ExperimentError::Profiling(format!(
            "{name} failed ({}): {}; artifacts: {}",
            output.status,
            String::from_utf8_lossy(tail).trim(),
            directory.display()
        )));
    }
    Ok(())
}

fn resolve(path: &Path) -> Result<PathBuf, ExperimentError> {
    if path.components().count() == 1
        && !path.is_absolute()
        && let Some(paths) = std::env::var_os("PATH")
    {
        for directory in std::env::split_paths(&paths) {
            let candidate = directory.join(path);
            if candidate.is_file() {
                return Ok(std::path::absolute(candidate)?);
            }
        }
    }
    if path.is_file() {
        return Ok(std::path::absolute(path)?);
    }
    Err(ExperimentError::Profiling(format!(
        "cannot find prerequisite {}; see docs/profiling.md",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        experiment::{ArtifactPolicy, RunContext},
        tools::yosys::{
            Yosys, YosysJob, YosysRunner,
            design::{Design, SourceFile},
        },
    };
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn diagnostic_recording_keeps_artifacts_without_adding_process_samples() {
        let root = tempfile::tempdir().unwrap();
        let perf = root.path().join("perf");
        fs::write(
            &perf,
            r#"#!/bin/sh
case "$1" in
record)
    while [ "$#" -gt 0 ]; do
        case "$1" in
        -o) shift; printf 'raw data' > "$1" ;;
        --) shift; "$@"; exit $? ;;
        esac
        shift
    done ;;
script) printf 'sampled stack\n' ;;
esac
"#,
        )
        .unwrap();
        fs::set_permissions(&perf, fs::Permissions::from_mode(0o755)).unwrap();
        let collapse = root.path().join("collapse.sh");
        let flamegraph = root.path().join("flamegraph.sh");
        fs::write(&collapse, "cat\n").unwrap();
        fs::write(&flamegraph, "printf '<svg>recording</svg>'\n").unwrap();
        let profiler = PerfProfiler {
            perf,
            perl: "/bin/sh".into(),
            addr2line: "/bin/true".into(),
            stackcollapse: collapse,
            flamegraph,
        };
        let yosys = root.path().join("yosys");
        fs::write(
            &yosys,
            "#!/bin/sh\ntest -f design.v || exit 3\nprintf 'called\\n' >> \"$0.calls\"\n",
        )
        .unwrap();
        fs::set_permissions(&yosys, fs::Permissions::from_mode(0o755)).unwrap();
        let mut context = RunContext::new(
            root.path().join("attempt"),
            ArtifactPolicy::new(false, false, false),
        )
        .with_perf(profiler);
        let runner = YosysRunner::new(&Yosys::new(yosys)).unwrap();
        let prepared = runner
            .prepare(
                &YosysJob {
                    design: Design {
                        files: vec![SourceFile::new("design.v", "module top; endmodule")],
                        ..Design::default()
                    },
                    assets: Vec::new(),
                    commands: vec!["stat".into()],
                    ..YosysJob::default()
                },
                &mut context,
            )
            .unwrap();
        runner.measure(&prepared, "measured", &mut context).unwrap();
        runner
            .diagnose(&prepared, "diagnostic", &mut context)
            .unwrap();
        assert_eq!(*context.processes(), 1);
        assert_eq!(
            fs::read_to_string(root.path().join("yosys.calls"))
                .unwrap()
                .lines()
                .count(),
            2
        );
        let directory = context.workspace().join("perf/diagnostic");
        assert!(directory.join("perf.data").is_file());
        assert_eq!(
            fs::read_to_string(directory.join("flamegraph.svg")).unwrap(),
            "<svg>recording</svg>"
        );
        assert!(directory.join("design.v").is_file());
        assert!(!context.workspace().join("scripts").exists());
    }
}
