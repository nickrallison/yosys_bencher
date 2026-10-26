use crate::{
    experiment::ExperimentError,
    tools::yosys::{Statistics, YosysRun},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmMeasurements {
    /// Yosys pass self CPU time, excluding subprocess startup and other passes.
    pub cpu_time: Duration,
    pub adds_subs: u64,
    pub negations: u64,
    pub multipliers: u64,
    pub depth: Option<u32>,
}

impl McmMeasurements {
    pub fn parse_run(run: &YosysRun) -> Result<Self, ExperimentError> {
        Self::parse(
            run.log("mcm")?,
            &run.statistics("mcm-stat")?,
            run.pass_time("mcm")?,
        )
    }

    pub fn parse(
        text: &str,
        stats: &Statistics,
        cpu_time: Duration,
    ) -> Result<Self, ExperimentError> {
        let count = |name: &str| stats.cell_count.get(name).copied().unwrap_or(0);
        let mut depth = None;
        for line in text
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("mcm:"))
        {
            if let Some((_, rest)) = line.split_once("adder(s), depth ") {
                let value = rest
                    .split(',')
                    .next()
                    .unwrap_or(rest)
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| ExperimentError::Parse("malformed MCM depth".into()))?;
                depth = Some(depth.map_or(value, |previous: u32| previous.max(value)));
            }
        }
        Ok(Self {
            cpu_time,
            adds_subs: count("$add") + count("$sub"),
            negations: count("$neg"),
            multipliers: count("$mul"),
            depth,
        })
    }
}

crate::benchmark_metrics! {
    McmMeasurements {
        scalar cpu_time(Duration, "MCM pass self CPU time", "s", true);
        scalar adds_subs(UInt, "Add/subtract operations", "operations", false);
        scalar negations(UInt, "Negations", "operations", false);
        scalar multipliers(UInt, "Remaining multipliers", "operations", false);
        optional depth(UInt, "MCM depth", "levels", false);
    }
}
