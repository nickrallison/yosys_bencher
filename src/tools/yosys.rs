use std::{
    collections::{HashMap, HashSet},
    fs,
    num::NonZeroU32,
    path::{Component, Path, PathBuf},
    process::Command,
    time::Duration,
};

use crate::experiment::{ExperimentError, ProcessOutput, RunContext, check_label};
use tempfile::TempDir;

pub mod design;
mod statistics;
use design::{Design, SourceFile};
pub use statistics::Statistics;

type Result<T> = std::result::Result<T, ExperimentError>;

#[derive(Debug, Clone, Getters)]
pub struct Yosys {
    #[get = "pub"]
    path: PathBuf,
}

impl Yosys {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(Debug, Clone, Default)]
pub struct YosysJob {
    pub design: Design,
    pub assets: Vec<SourceFile>,
    pub commands: Vec<String>,
    /// Labels of logs produced by `tee`, available even when artifacts are disabled.
    pub logs: Vec<String>,
}

impl YosysJob {
    /// Capture a stage without suppressing its output in the complete process log.
    pub fn tee(&mut self, label: &str, commands: &[String]) -> Result<()> {
        check_label(label)?;
        if commands.is_empty() || self.logs.iter().any(|name| name == label) {
            return Err(ExperimentError::Config(format!(
                "invalid or duplicate log stage {label}"
            )));
        }
        for (index, command) in commands.iter().enumerate() {
            let mode = if index == 0 { "-o" } else { "-a" };
            self.commands
                .push(format!("tee {mode} .yosys-logs/{label}.log {command}"));
        }
        self.logs.push(label.into());
        Ok(())
    }
}

#[derive(Debug)]
pub struct PreparedYosysJob {
    directory: TempDir,
    script: String,
    files: Vec<PathBuf>,
    includes: Vec<(PathBuf, PathBuf)>,
    logs: Vec<String>,
}

pub struct YosysRunner {
    executable: PathBuf,
}

impl YosysRunner {
    pub fn new(yosys: &Yosys) -> Result<Self> {
        Ok(Self {
            executable: super::executable(yosys.path())?,
        })
    }

    pub fn prepare(&self, job: &YosysJob, context: &mut RunContext) -> Result<PreparedYosysJob> {
        fs::create_dir_all(context.workspace())?;
        let directory = tempfile::Builder::new()
            .prefix(".job-")
            .tempdir_in(context.workspace())?;
        let mut files = Vec::new();
        let mut seen = HashSet::from([PathBuf::from("run.ys"), PathBuf::from("inspect.ys")]);
        for file in job
            .design
            .files
            .iter()
            .chain(&job.design.headers)
            .chain(&job.assets)
        {
            let path = relative_path(&file.path, false)?;
            if path.starts_with(".yosys-includes")
                || path.starts_with(".yosys-logs")
                || !seen.insert(path.clone())
            {
                return Err(ExperimentError::Config(format!(
                    "conflicting job path {}",
                    path.display()
                )));
            }
            let target = directory.path().join(&path);
            fs::create_dir_all(target.parent().expect("file has a parent"))?;
            fs::write(target, &file.source)?;
            files.push(path);
        }
        let mut includes = Vec::new();
        for (index, path) in job.design.options.include_dirs.iter().enumerate() {
            let path = relative_path(path, true)?;
            if path.starts_with(".yosys-includes") || path.starts_with(".yosys-logs") {
                return Err(ExperimentError::Config(format!(
                    "reserved include path {}",
                    path.display()
                )));
            }
            fs::create_dir_all(directory.path().join(&path))?;
            let alias = PathBuf::from(format!(".yosys-includes/{index}"));
            let target = PathBuf::from("..").join(path);
            includes.push((alias, target));
        }
        stage_includes(directory.path(), &includes)?;
        for label in &job.logs {
            check_label(label)?;
        }
        fs::create_dir(directory.path().join(".yosys-logs"))?;
        let mut commands = frontend_commands(&job.design)?;
        commands.extend(job.commands.iter().cloned());
        let script = format!("{}\n", commands.join("\n"));
        fs::write(directory.path().join("run.ys"), &script)?;
        files.push("run.ys".into());
        let prepared = PreparedYosysJob {
            directory,
            script,
            files,
            includes,
            logs: job.logs.clone(),
        };
        if *context.artifacts().scripts() {
            let destination = prepared.retained_scripts(context.workspace());
            prepared.copy_to(&destination)?;
        }
        Ok(prepared)
    }

    pub fn measure(
        &self,
        job: &PreparedYosysJob,
        label: &str,
        context: &mut RunContext,
    ) -> Result<ProcessOutput> {
        self.run(job, "run.ys", label, false, context)
    }

    /// Yosys reports per-pass self CPU time, distinct from process wall time.
    pub fn measure_with_timings(
        &self,
        job: &PreparedYosysJob,
        label: &str,
        context: &mut RunContext,
    ) -> Result<(ProcessOutput, HashMap<String, Duration>)> {
        #[derive(serde::Deserialize)]
        struct Timing {
            runtime_ns: u64,
        }
        #[derive(serde::Deserialize)]
        struct Report {
            passes: HashMap<String, Timing>,
        }

        let output = self.run(job, "run.ys", label, true, context)?;
        let report: Report = serde_json::from_slice(&fs::read(
            job.directory.path().join(".yosys-logs/timing.json"),
        )?)
        .map_err(|error| ExperimentError::Parse(format!("invalid Yosys pass timings: {error}")))?;
        Ok((
            output,
            report
                .passes
                .into_iter()
                .map(|(name, timing)| (name, Duration::from_nanos(timing.runtime_ns)))
                .collect(),
        ))
    }

    fn run(
        &self,
        job: &PreparedYosysJob,
        script: &str,
        label: &str,
        timings: bool,
        context: &mut RunContext,
    ) -> Result<ProcessOutput> {
        check_label(label)?;
        // A failed repetition must never expose logs from an earlier execution.
        let directory = job.directory.path().join(".yosys-logs");
        fs::remove_dir_all(&directory)?;
        fs::create_dir(&directory)?;
        let result = context.run_process(
            label,
            &mut self.command(job.directory.path(), script, timings),
        );
        let retained = (|| -> Result<()> {
            if *context.artifacts().logs() {
                for entry in fs::read_dir(&directory)? {
                    let entry = entry?;
                    let destination = context.workspace().join("logs").join(label);
                    fs::create_dir_all(&destination)?;
                    fs::copy(entry.path(), destination.join(entry.file_name()))?;
                }
            }
            Ok(())
        })();
        let output = result?;
        retained?;
        for stage in &job.logs {
            if !directory.join(format!("{stage}.log")).is_file() {
                return Err(ExperimentError::Parse(format!(
                    "missing Yosys stage log {stage}"
                )));
            }
        }
        Ok(output)
    }

    /// Additional invocations only. Call after all measured phases in a comparison.
    pub fn diagnose(
        &self,
        job: &PreparedYosysJob,
        label: &str,
        context: &mut RunContext,
    ) -> Result<()> {
        self.diagnose_repeated(job, label, NonZeroU32::new(1).unwrap(), context)
    }

    /// Retain one netlist per phase and one perf recording per repetition.
    pub fn diagnose_repeated(
        &self,
        job: &PreparedYosysJob,
        label: &str,
        repetitions: NonZeroU32,
        context: &mut RunContext,
    ) -> Result<()> {
        check_label(label)?;
        if *context.artifacts().netlists() {
            let directory = context.workspace().join("netlists");
            fs::create_dir_all(&directory)?;
            let path = directory.canonicalize()?.join(format!("{label}.v"));
            let script = format!("{}write_verilog {}\n", job.script, quote_path(&path)?);
            fs::write(job.directory.path().join("inspect.ys"), &script)?;
            if *context.artifacts().scripts() {
                fs::write(
                    job.retained_scripts(context.workspace())
                        .join(format!("inspect-{label}.ys")),
                    script,
                )?;
            }
            self.run(
                job,
                "inspect.ys",
                &format!("inspect-{label}"),
                false,
                context,
            )?;
            if !path.is_file() {
                return Err(ExperimentError::Parse(format!(
                    "missing diagnostic netlist {}",
                    path.display()
                )));
            }
        }
        if let Some(perf) = context.perf() {
            for index in 0..repetitions.get() {
                let label = if repetitions.get() == 1 {
                    label.to_owned()
                } else {
                    format!("{label}-{index}")
                };
                let directory = context.workspace().join("perf").join(label);
                job.copy_to(&directory)?;
                perf.record(
                    &self.command(&directory.canonicalize()?, "run.ys", false),
                    &directory,
                )?;
            }
        }
        Ok(())
    }

    fn command(&self, directory: &Path, script: &str, timings: bool) -> Command {
        let mut command = Command::new(&self.executable);
        command.current_dir(directory).arg("-Q");
        if timings {
            command.args(["--perffile", ".yosys-logs/timing.json"]);
        } else {
            command.arg("-T");
        }
        command.args(["-s", script]);
        command
    }
}

impl PreparedYosysJob {
    pub fn read_log(&self, label: &str) -> Result<String> {
        check_label(label)?;
        if !self.logs.iter().any(|name| name == label) {
            return Err(ExperimentError::Config(format!(
                "unknown Yosys log stage {label}"
            )));
        }
        Ok(fs::read_to_string(
            self.directory
                .path()
                .join(format!(".yosys-logs/{label}.log")),
        )?)
    }

    fn retained_scripts(&self, workspace: &Path) -> PathBuf {
        workspace.join("scripts").join(
            self.directory
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .trim_start_matches('.'),
        )
    }

    fn copy_to(&self, destination: &Path) -> Result<()> {
        for path in &self.files {
            let target = destination.join(path);
            fs::create_dir_all(target.parent().expect("file has a parent"))?;
            fs::copy(self.directory.path().join(path), target)?;
        }
        stage_includes(destination, &self.includes)?;
        fs::create_dir_all(destination.join(".yosys-logs"))?;
        Ok(())
    }
}

fn stage_includes(directory: &Path, includes: &[(PathBuf, PathBuf)]) -> Result<()> {
    for (alias, target) in includes {
        let alias = directory.join(alias);
        fs::create_dir_all(alias.parent().unwrap())?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, alias)?;
        #[cfg(not(unix))]
        return Err(ExperimentError::Unsupported(
            "staging include directories requires Unix symlinks".into(),
        ));
    }
    Ok(())
}

fn relative_path(path: &Path, allow_root: bool) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(name) => normalized.push(name),
            Component::CurDir => {}
            _ => {
                return Err(ExperimentError::Config(format!(
                    "job path must stay inside the workspace: {}",
                    path.display()
                )));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        if allow_root {
            return Ok(".".into());
        }
        return Err(ExperimentError::Config("empty job file path".into()));
    }
    Ok(normalized)
}

pub fn quote(value: &str) -> Result<String> {
    if value.chars().any(char::is_control) {
        return Err(ExperimentError::Config(
            "control character in Yosys argument".into(),
        ));
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

pub fn quote_path(path: &Path) -> Result<String> {
    quote(
        path.to_str()
            .ok_or_else(|| ExperimentError::Config("Yosys paths must be UTF-8".into()))?,
    )
}

fn frontend_commands(design: &Design) -> Result<Vec<String>> {
    if design.files.is_empty() {
        return Err(ExperimentError::Config("design has no source files".into()));
    }
    let mut options = Vec::new();
    if design.options.system_verilog {
        options.push("-sv".into());
    }
    for (name, value) in &design.options.defines {
        let value = value
            .as_ref()
            .map(|value| format!("={value}"))
            .unwrap_or_default();
        options.push(argument(&format!("-D{name}{value}"))?);
    }
    for index in 0..design.options.include_dirs.len() {
        options.push(format!("-I.yosys-includes/{index}"));
    }
    for option in &design.options.extra_args {
        options.push(argument(option)?);
    }
    let files = design
        .files
        .iter()
        .map(|file| quote_path(&relative_path(&file.path, false)?))
        .collect::<Result<Vec<_>>>()?;
    Ok(vec![format!(
        "read_verilog {} {}",
        options.join(" "),
        files.join(" ")
    )])
}

pub fn hierarchy_commands(design: &Design) -> Result<Vec<String>> {
    let mut command = match &design.top {
        Some(top) => format!("hierarchy -top {}", argument(top)?),
        None => "hierarchy -auto-top".into(),
    };
    for (name, value) in &design.parameters {
        command.push_str(&format!(
            " -chparam {} {}",
            argument(name)?,
            parameter_argument(value)?
        ));
    }
    Ok(vec![command])
}

#[cfg(test)]
mod tests;

/// Yosys retains quotes in pass arguments; only filenames are unquoted by the frontend.
pub fn argument(value: &str) -> Result<String> {
    if value.is_empty()
        || value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ';' || c == '#')
    {
        return Err(ExperimentError::Config(format!(
            "invalid Yosys argument {value:?}"
        )));
    }
    Ok(value.into())
}

fn parameter_argument(value: &str) -> Result<String> {
    if value.starts_with('"')
        && value.ends_with('"')
        && value.len() >= 2
        && !value.chars().any(char::is_control)
    {
        return Ok(value.into());
    }
    argument(value)
}
