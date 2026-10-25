use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
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

#[derive(Debug, Clone)]
pub struct YosysStage {
    pub name: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct YosysJob {
    pub design: Design,
    pub assets: Vec<SourceFile>,
    pub stages: Vec<YosysStage>,
    /// Collect Yosys pass self CPU times in addition to process wall time.
    pub timings: bool,
}

impl YosysJob {
    pub fn stage(&mut self, name: &str, commands: Vec<String>) -> Result<()> {
        check_label(name)?;
        if commands.is_empty() || self.stages.iter().any(|stage| stage.name == name) {
            return Err(ExperimentError::Config(format!(
                "invalid or duplicate stage {name}"
            )));
        }
        self.stages.push(YosysStage {
            name: name.into(),
            commands,
        });
        Ok(())
    }
}

/// Captured data belongs to this execution, independent of later runs of the job.
#[derive(Debug)]
pub struct YosysRun {
    pub process: ProcessOutput,
    pub stage_logs: BTreeMap<String, String>,
    pub pass_timings: HashMap<String, Duration>,
}

impl YosysRun {
    pub fn log(&self, name: &str) -> Result<&str> {
        self.stage_logs
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| ExperimentError::Parse(format!("missing Yosys stage log {name}")))
    }

    pub fn statistics(&self, name: &str) -> Result<Statistics> {
        Statistics::parse(self.log(name)?)
    }
}

#[derive(Debug)]
pub struct PreparedYosysJob {
    directory: TempDir,
    script: String,
    files: Vec<PathBuf>,
    includes: Vec<(PathBuf, PathBuf)>,
    stages: Vec<String>,
    timings: bool,
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
        fs::create_dir(directory.path().join(".yosys-logs"))?;
        let mut commands = frontend_commands(&job.design)?;
        let mut stages = HashSet::new();
        for stage in &job.stages {
            check_label(&stage.name)?;
            if stage.commands.is_empty() || !stages.insert(&stage.name) {
                return Err(ExperimentError::Config(format!(
                    "invalid or duplicate stage {}",
                    stage.name
                )));
            }
            for (index, command) in stage.commands.iter().enumerate() {
                let mode = if index == 0 { "-o" } else { "-a" };
                commands.push(format!(
                    "tee {mode} .yosys-logs/{}.log {command}",
                    stage.name
                ));
            }
        }
        let script = format!("{}\n", commands.join("\n"));
        fs::write(directory.path().join("run.ys"), &script)?;
        files.push("run.ys".into());
        let prepared = PreparedYosysJob {
            directory,
            script,
            files,
            includes,
            stages: job.stages.iter().map(|stage| stage.name.clone()).collect(),
            timings: job.timings,
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
    ) -> Result<YosysRun> {
        #[derive(serde::Deserialize)]
        struct Timing {
            runtime_ns: u64,
        }
        #[derive(serde::Deserialize)]
        struct Report {
            passes: HashMap<String, Timing>,
        }

        let process = self.run(job, "run.ys", label, job.timings, context)?;
        let stage_logs = job
            .stages
            .iter()
            .map(|name| {
                Ok((
                    name.clone(),
                    fs::read_to_string(
                        job.directory.path().join(format!(".yosys-logs/{name}.log")),
                    )?,
                ))
            })
            .collect::<Result<_>>()?;
        let pass_timings = if job.timings {
            let report: Report = serde_json::from_slice(&fs::read(
                job.directory.path().join(".yosys-logs/timing.json"),
            )?)
            .map_err(|error| {
                ExperimentError::Parse(format!("invalid Yosys pass timings: {error}"))
            })?;
            report
                .passes
                .into_iter()
                .map(|(name, timing)| (name, Duration::from_nanos(timing.runtime_ns)))
                .collect()
        } else {
            HashMap::new()
        };
        Ok(YosysRun {
            process,
            stage_logs,
            pass_timings,
        })
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
        context.record_diagnostic(label, retained);
        let output = result?;
        for stage in &job.stages {
            if !directory.join(format!("{stage}.log")).is_file() {
                return Err(ExperimentError::Parse(format!(
                    "missing Yosys stage log {stage}"
                )));
            }
        }
        Ok(output)
    }

    /// Extra invocations only; failures are diagnostics, never measurement failures.
    pub fn diagnose(&self, job: &PreparedYosysJob, label: &str, context: &mut RunContext) {
        if *context.artifacts().netlists() {
            let result = self.netlist(job, label, context);
            context.record_diagnostic(&format!("{label}-netlist"), result);
        }
        if let Some(perf) = context.perf().clone() {
            let result = (|| -> Result<()> {
                check_label(label)?;
                let directory = context.workspace().join("perf").join(label);
                job.copy_to(&directory)?;
                perf.record(
                    &self.command(&directory.canonicalize()?, "run.ys", false),
                    &directory,
                )
            })();
            context.record_diagnostic(&format!("{label}-perf"), result);
        }
    }

    fn netlist(&self, job: &PreparedYosysJob, label: &str, context: &mut RunContext) -> Result<()> {
        check_label(label)?;
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
