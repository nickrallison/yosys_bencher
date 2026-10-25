use crate::experiment::ExperimentError;
use serde::Deserialize;
use std::collections::HashMap;

type Result<T> = std::result::Result<T, ExperimentError>;

#[derive(Debug, Clone, PartialEq)]
pub struct Statistics {
    pub cell_count: HashMap<String, u64>,
    pub area: Option<f64>,
}

impl Statistics {
    /// Whole-design statistics from a dedicated `stat -json` capture.
    pub fn parse(text: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct Counts {
            num_cells: u64,
            num_cells_by_type: HashMap<String, u64>,
            area: Option<f64>,
        }
        #[derive(Deserialize)]
        struct Report {
            modules: HashMap<String, Counts>,
            design: Option<Counts>,
        }
        let invalid = |message: &str| ExperimentError::Parse(message.into());
        let report: Report = serde_json::from_str(text).map_err(|error| {
            ExperimentError::Parse(format!("invalid Yosys statistics: {error}"))
        })?;
        let counts = match report.design.as_ref() {
            Some(counts) => counts,
            None if report.modules.len() == 1 => report.modules.values().next().unwrap(),
            None => {
                return Err(invalid(
                    "missing whole-design statistics; select a top module",
                ));
            }
        };
        let sum = |values: &HashMap<String, u64>| {
            values
                .values()
                .try_fold(0_u64, |sum, count| sum.checked_add(*count))
                .ok_or_else(|| invalid("cell count overflow"))
        };
        // Yosys includes module instances in the type table. Count only leaf cells.
        let cell_count: HashMap<_, _> = counts
            .num_cells_by_type
            .iter()
            .filter(|(name, _)| {
                !report
                    .modules
                    .keys()
                    .any(|module| module.trim_start_matches('\\') == name.trim_start_matches('\\'))
            })
            .map(|(name, count)| (name.clone(), *count))
            .collect();
        // Older Yosys includes module instances in num_cells; newer versions exclude them.
        if counts.num_cells != sum(&cell_count)?
            && counts.num_cells != sum(&counts.num_cells_by_type)?
        {
            return Err(invalid("cell table does not match its total"));
        }
        if counts
            .area
            .is_some_and(|area| !area.is_finite() || area < 0.0)
        {
            return Err(invalid("invalid chip area"));
        }
        Ok(Self {
            cell_count,
            area: counts.area,
        })
    }
}
