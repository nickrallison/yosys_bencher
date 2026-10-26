use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use contracts::contract_trait;
use serde::{Deserialize, Serialize};

use crate::{
    experiment::{BenchmarkOut, ExperimentError},
    metrics::{MetricDescriptor, MetricType, MetricValue},
};

const EVENTS: &[(&str, &str)] = &[
    ("task_clock", "task-clock"),
    ("cycles", "cycles"),
    ("instructions", "instructions"),
    ("branches", "branches"),
    ("branch_misses", "branch-misses"),
    ("cache_references", "cache-references"),
    ("cache_misses", "cache-misses"),
    ("context_switches", "context-switches"),
    ("cpu_migrations", "cpu-migrations"),
    ("page_faults", "page-faults"),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfCounter {
    pub event: String,
    pub value: Option<f64>,
    pub unit: String,
    pub runtime: Duration,
    pub running_percent: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfMeasurements {
    pub counters: Vec<PerfCounter>,
}

impl PerfMeasurements {
    pub fn parse(text: &str) -> Result<Self, ExperimentError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct Counter {
            event: String,
            counter_value: String,
            unit: String,
            event_runtime: u64,
            pcnt_running: f64,
        }
        let invalid = || ExperimentError::Parse("invalid perf stat counters".into());
        let mut counters = Vec::new();
        for line in text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let raw: Counter = serde_json::from_str(line).map_err(|_| invalid())?;
            let value = match raw.counter_value.as_str() {
                "<not supported>" | "<not counted>" => None,
                text => {
                    let value: f64 = text.parse().map_err(|_| invalid())?;
                    if !value.is_finite() || value < 0.0 {
                        return Err(invalid());
                    }
                    Some(value)
                }
            };
            if !raw.pcnt_running.is_finite() || !(0.0..=100.0).contains(&raw.pcnt_running) {
                return Err(invalid());
            }
            counters.push(PerfCounter {
                event: raw.event,
                value,
                unit: raw.unit,
                runtime: Duration::from_nanos(raw.event_runtime),
                running_percent: raw.pcnt_running,
            });
        }
        if counters.is_empty() {
            return Err(invalid());
        }
        Ok(Self { counters })
    }

    fn value(&self, event: &str) -> Option<f64> {
        let mut values = self
            .counters
            .iter()
            .filter(|counter| {
                counter.event == event
                    || counter
                        .event
                        .strip_suffix('/')
                        .and_then(|name| name.rsplit_once('/'))
                        .is_some_and(|(_, name)| name == event)
            })
            .filter_map(|counter| counter.value);
        Some(values.next()? + values.sum::<f64>())
    }
}

#[contract_trait]
impl BenchmarkOut for PerfMeasurements {
    fn list_metrics() -> Vec<MetricDescriptor> {
        EVENTS
            .iter()
            .map(|&(name, _)| {
                let (kind, unit) = if name == "task_clock" {
                    (MetricType::Duration, "s")
                } else {
                    (MetricType::Float, "events")
                };
                MetricDescriptor::new(name, name, kind, unit, true)
            })
            .collect()
    }

    fn get_metric(&self, name: &str) -> Option<MetricValue> {
        let (_, event) = EVENTS.iter().find(|&&(key, _)| key == name)?;
        let value = self.value(event)?;
        if name == "task_clock" {
            Duration::try_from_secs_f64(value / 1000.0)
                .ok()
                .map(MetricValue::Duration)
        } else {
            Some(MetricValue::Float(value))
        }
    }
}

#[derive(Debug, Clone)]
pub struct PerfStat {
    executable: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PreparedPerfStat {
    executable: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub enum PerfAvailability {
    #[default]
    Disabled,
    Available(PreparedPerfStat),
    Unavailable(String),
}

impl PerfAvailability {
    pub fn prepare(perf: Option<&PerfStat>) -> Self {
        match perf.map(PerfStat::prepare) {
            None => Self::Disabled,
            Some(Ok(perf)) => Self::Available(perf),
            Some(Err(error)) => Self::Unavailable(error.to_string()),
        }
    }
}

impl Default for PerfStat {
    fn default() -> Self {
        Self::new("perf")
    }
}

impl PerfStat {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
        }
    }

    pub fn prepare(&self) -> Result<PreparedPerfStat, ExperimentError> {
        if !cfg!(target_os = "linux") {
            return Err(ExperimentError::Profiling(
                "perf counters require Linux".into(),
            ));
        }
        let prepared = PreparedPerfStat {
            executable: crate::tools::executable(&self.executable)?,
        };
        let report = tempfile::NamedTempFile::new()?;
        let output = prepared
            .command(&Command::new("true"), report.path())
            .output()?;
        if !output.status.success() {
            return Err(ExperimentError::Profiling(
                String::from_utf8_lossy(&output.stderr).trim().into(),
            ));
        }
        let measurements = PerfMeasurements::parse(&fs::read_to_string(report.path())?)?;
        if measurements
            .counters
            .iter()
            .all(|counter| counter.value.is_none())
        {
            return Err(ExperimentError::Profiling("no usable perf counters".into()));
        }
        Ok(prepared)
    }
}

impl PreparedPerfStat {
    pub(crate) fn command(&self, command: &Command, report: &Path) -> Command {
        let mut wrapped = Command::new(&self.executable);
        wrapped
            .args(["stat", "--json-output", "--no-big-num", "-e"])
            .arg(
                EVENTS
                    .iter()
                    .map(|(_, event)| *event)
                    .collect::<Vec<_>>()
                    .join(","),
            )
            .arg("-o")
            .arg(report)
            .arg("--")
            .arg(command.get_program())
            .args(command.get_args());
        if let Some(directory) = command.get_current_dir() {
            wrapped.current_dir(directory);
        }
        super::copy_environment(command, &mut wrapped);
        wrapped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::{ArtifactPolicy, RunContext, RunOptions};
    use crate::test_support::{required, tool};

    fn collector() -> PerfAvailability {
        PerfAvailability::Available(required(
            PerfStat::new(tool("perf", Some("PERF"))).prepare(),
        ))
    }

    #[test]
    fn counters_are_enabled_by_default_and_preserve_process_output_and_environment() {
        assert!(RunOptions::default().perf_counters.is_some());
        let root = tempfile::tempdir().unwrap();
        let mut context = RunContext::new(
            root.path().join("workspace"),
            ArtifactPolicy::new(true, false, false),
        )
        .with_perf_counters(collector());
        let output = context.run_process("measured", Command::new(tool("sh", Some("SH")))
            .args(["-c", "test \"$BENCH_PERF_TEST\" = 'kept value' || exit 3; test \"${RUST_BACKTRACE+x}\" != x || exit 4; printf child-output; printf child-error >&2; echo ran >> executions"])
            .env("BENCH_PERF_TEST", "kept value").env_remove("RUST_BACKTRACE")).unwrap();
        assert_eq!(output.stdout().unwrap(), "child-output");
        assert_eq!(output.output.stderr, b"child-error");
        let perf = output.perf.unwrap();
        assert!(
            matches!(perf.get_metric("task_clock"), Some(MetricValue::Duration(time)) if time > Duration::ZERO)
        );
        assert!(
            perf.counters
                .iter()
                .all(|counter| (0.0..=100.0).contains(&counter.running_percent))
        );
        assert_eq!(
            fs::read_to_string(context.workspace().join("executions")).unwrap(),
            "ran\n"
        );
        assert_eq!(*context.processes(), 1);
        assert!(
            context.diagnostics().is_empty(),
            "{:?}",
            context.diagnostics()
        );
        assert!(
            context
                .workspace()
                .join("process-000-measured.perf.json")
                .is_file()
        );
        assert_eq!(fs::read_dir(context.workspace()).unwrap().count(), 5);
    }

    #[test]
    fn unavailable_and_disabled_counters_do_not_rerun_workloads() {
        let root = tempfile::tempdir().unwrap();
        for (name, perf) in [
            (
                "unavailable",
                Some(PerfStat::new(root.path().join("missing-perf"))),
            ),
            ("disabled", None),
        ] {
            let mut context = RunContext::new(
                root.path().join(name),
                ArtifactPolicy::new(false, false, false),
            )
            .with_perf_counters(PerfAvailability::prepare(perf.as_ref()));
            for _ in 0..2 {
                let output = context
                    .run_process(
                        "measured",
                        Command::new(tool("sh", Some("SH"))).args(["-c", "echo ran >> executions"]),
                    )
                    .unwrap();
                assert!(output.perf.is_none());
            }
            assert_eq!(
                fs::read_to_string(context.workspace().join("executions")).unwrap(),
                "ran\nran\n"
            );
            assert_eq!(
                context.diagnostics().len(),
                usize::from(name == "unavailable")
            );
            assert_eq!(fs::read_dir(context.workspace()).unwrap().count(), 1);
        }
    }

    #[test]
    fn instrumented_process_failures_keep_status_and_are_not_retried() {
        let root = tempfile::tempdir().unwrap();
        let mut context = RunContext::new(
            root.path().join("workspace"),
            ArtifactPolicy::new(false, false, false),
        )
        .with_perf_counters(collector());
        let result = context.run_process(
            "failing",
            Command::new(tool("sh", Some("SH")))
                .args(["-c", "echo ran >> executions; echo failure >&2; exit 7"]),
        );
        assert!(
            matches!(result, Err(ExperimentError::Process { status, detail, .. }) if status.code() == Some(7) && detail == "failure")
        );
        assert_eq!(
            fs::read_to_string(context.workspace().join("executions")).unwrap(),
            "ran\n"
        );
        assert!(
            context.diagnostics().is_empty(),
            "{:?}",
            context.diagnostics()
        );
    }

    #[test]
    fn parsing_retains_missing_counters_and_sums_hybrid_pmus() {
        let text = "{\"counter-value\":\"10.5\",\"unit\":\"\",\"event\":\"cpu_core/cycles/\",\"event-runtime\":100,\"pcnt-running\":50}\n{\"counter-value\":\"20\",\"unit\":\"\",\"event\":\"cpu_atom/cycles/\",\"event-runtime\":100,\"pcnt-running\":100}\n{\"counter-value\":\"<not counted>\",\"unit\":\"\",\"event\":\"instructions\",\"event-runtime\":0,\"pcnt-running\":0}";
        let perf = PerfMeasurements::parse(text).unwrap();
        assert_eq!(perf.get_metric("cycles"), Some(MetricValue::Float(30.5)));
        assert_eq!(perf.get_metric("instructions"), None);
        assert_eq!(perf.counters[2].value, None);
        let decoded: PerfMeasurements =
            serde_json::from_str(&serde_json::to_string(&perf).unwrap()).unwrap();
        assert_eq!(decoded, perf);
        for invalid in [
            "",
            "{}",
            &text.replace("10.5", "NaN"),
            &text.replace("10.5", "-1"),
            &text.replace("\"pcnt-running\":50", "\"pcnt-running\":101"),
        ] {
            assert!(PerfMeasurements::parse(invalid).is_err());
        }
    }
}
