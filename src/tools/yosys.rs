use std::{
    collections::{BTreeMap, HashSet},
    path::{Component, Path, PathBuf},
    process::Command,
    time::Duration,
};

use crate::experiment::{ExperimentError, ProcessOutput, RunContext, check_label};

mod capture;
pub mod design;
pub(crate) mod instrumentation;
mod plan;
mod statistics;
mod timings;
mod workspace;
pub use capture::YosysCapture;
use design::{Design, SourceFile};
pub use plan::YosysPlan;
pub use statistics::Statistics;
pub use timings::{PassCall, PassTiming, YosysTimings};
pub use workspace::PreparedYosysJob;

type Result<T> = std::result::Result<T, ExperimentError>;

#[derive(Debug, Clone, Getters)]
pub struct Yosys {
    #[get = "pub"]
    path: PathBuf,
    call_trace: bool,
}

impl Yosys {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            call_trace: false,
        }
    }

    /// Require the call trace emitted by a benchmark-instrumented Yosys build.
    pub fn with_call_trace(mut self) -> Self {
        self.call_trace = true;
        self
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
}

impl YosysJob {
    pub fn new(design: Design) -> Result<Self> {
        let commands = hierarchy_commands(&design)?;
        let mut job = Self {
            design,
            ..Self::default()
        };
        job.stage("hierarchy", commands)?;
        Ok(job)
    }

    pub fn stage_with_statistics(&mut self, name: &str, commands: Vec<String>) -> Result<()> {
        self.stage(name, commands)?;
        self.stage(&format!("{name}-stat"), vec!["stat -json".into()])
    }

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

#[derive(Debug)]
pub struct YosysRun {
    pub process: ProcessOutput,
    pub stage_logs: BTreeMap<String, String>,
    pub timings: YosysTimings,
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

    pub fn pass_time(&self, name: &str) -> Result<Duration> {
        self.timings
            .passes
            .get(name)
            .map(|pass| pass.runtime)
            .ok_or_else(|| ExperimentError::Parse(format!("missing Yosys pass timing {name}")))
    }
}

pub struct YosysRunner {
    executable: PathBuf,
    call_trace: bool,
}

impl YosysRunner {
    pub fn new(yosys: &Yosys) -> Result<Self> {
        Ok(Self {
            executable: super::executable(yosys.path())?,
            call_trace: yosys.call_trace,
        })
    }

    pub fn prepare(&self, job: &YosysJob, context: &mut RunContext) -> Result<PreparedYosysJob> {
        self.materialize(job.compile()?, context)
    }

    pub fn materialize(
        &self,
        plan: YosysPlan,
        context: &mut RunContext,
    ) -> Result<PreparedYosysJob> {
        let prepared = PreparedYosysJob::materialize(plan, context.workspace())?;
        if *context.artifacts().scripts() {
            prepared.copy_to(&prepared.retained_scripts(context.workspace()))?;
        }
        Ok(prepared)
    }

    pub fn measure(
        &self,
        job: &PreparedYosysJob,
        label: &str,
        context: &mut RunContext,
    ) -> Result<YosysRun> {
        let captured = self.capture(job, "run.ys", label, true, context)?;
        if self.call_trace && !captured.files.contains_key(Path::new("calls.json")) {
            return Err(ExperimentError::Parse("missing Yosys call trace".into()));
        }
        YosysRun::decode(&job.plan, captured)
    }

    fn capture(
        &self,
        job: &PreparedYosysJob,
        script: &str,
        label: &str,
        timings: bool,
        context: &mut RunContext,
    ) -> Result<YosysCapture> {
        check_label(label)?;
        job.reset_logs()?;
        let process = context.run_process(
            label,
            &mut self.command(job.directory.path(), script, timings),
        );
        let files = job.capture_logs();
        if *context.artifacts().logs() {
            let retained = files
                .as_ref()
                .map_err(|error| ExperimentError::Parse(error.to_string()))
                .and_then(|files| {
                    context
                        .artifact_writer()
                        .write_files(&PathBuf::from("logs").join(label), files)
                });
            context.record_diagnostic(label, retained);
        }
        let process = process.map_err(command_error)?;
        Ok(YosysCapture {
            process,
            files: files?,
        })
    }

    pub fn diagnose(&self, job: &PreparedYosysJob, label: &str, context: &mut RunContext) {
        if *context.artifacts().netlists() {
            let result = self.netlist(job, label, context);
            context.record_diagnostic(&format!("{label}-netlist"), result);
        }
        if let Some(perf) = context.perf().clone() {
            context.progress().detail(format!("Profiling {label}"));
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

    fn command(&self, directory: &Path, script: &str, timings: bool) -> Command {
        let mut command = Command::new(&self.executable);
        command.current_dir(directory).arg("-Q");
        if timings {
            command.args(["--perffile", ".yosys-logs/timing.json"]);
            command.env("YOSYS_BENCH_CALL_TRACE", ".yosys-logs/calls.json");
        } else {
            command.arg("-T");
            command.env_remove("YOSYS_BENCH_CALL_TRACE");
        }
        command.args(["-s", script]);
        command
    }
}

fn command_error(error: ExperimentError) -> ExperimentError {
    if let ExperimentError::Process { stdout, stderr, .. } = &error {
        for bytes in [stdout, stderr] {
            let text = String::from_utf8_lossy(bytes);
            if let Some((_, rest)) = text.split_once("No such command:")
                && let Some(name) = rest.split_whitespace().next()
            {
                return ExperimentError::Unsupported(format!(
                    "this Yosys revision does not provide the {name} pass"
                ));
            }
        }
    }
    error
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
