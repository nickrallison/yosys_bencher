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
