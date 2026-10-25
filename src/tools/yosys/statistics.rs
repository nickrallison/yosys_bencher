use crate::experiment::ExperimentError;
use std::collections::{HashMap, HashSet};

type Result<T> = std::result::Result<T, ExperimentError>;
fn invalid(message: &str) -> ExperimentError {
    ExperimentError::Parse(message.into())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Statistics {
    pub top: String,
    pub cell_count: HashMap<String, u64>,
    pub area: Option<f64>,
}

impl Statistics {
    /// Select the final statistics block for the requested or inferred top.
    pub fn parse(text: &str, top: Option<&str>) -> Result<Self> {
        let mut headers = Vec::new();
        let mut position = 0;
        for line in text.split_inclusive('\n') {
            if let Some(name) = line
                .trim()
                .strip_prefix("=== ")
                .and_then(|s| s.strip_suffix(" ==="))
            {
                headers.push((
                    name.trim_start_matches('\\'),
                    position + line.len(),
                    position,
                ));
            }
            position += line.len();
        }
        let inferred = text
            .lines()
            .filter_map(|line| line.trim().strip_prefix("Top module:"))
            .next_back()
            .map(|name| name.trim().trim_start_matches('\\'));
        let top = if let Some(top) = top.or(inferred) {
            top.trim_start_matches('\\')
        } else {
            let names: HashSet<_> = headers
                .iter()
                .map(|(name, _, _)| *name)
                .filter(|name| *name != "design hierarchy")
                .collect();
            if names.len() != 1 {
                return Err(invalid("missing or ambiguous top-module statistics"));
            }
            names.into_iter().next().unwrap()
        };
        let index = headers
            .iter()
            .rposition(|(name, _, _)| *name == top)
            .ok_or_else(|| invalid("missing top-module statistics"))?;
        let block = &text[headers[index].1
            ..headers
                .get(index + 1)
                .map(|header| header.2)
                .unwrap_or(text.len())];
        let cell_count = cells(block)?;
        let mut area = None;
        for line in block
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("Chip area for "))
        {
            let value: f64 = line
                .rsplit_once(':')
                .ok_or_else(|| invalid("malformed chip area"))?
                .1
                .trim()
                .parse()
                .map_err(|_| invalid("malformed chip area"))?;
            if !value.is_finite() || value < 0.0 || area.replace(value).is_some() {
                return Err(invalid("invalid or duplicate chip area"));
            }
        }
        Ok(Self {
            top: top.into(),
            cell_count,
            area,
        })
    }
}

fn cells(block: &str) -> Result<HashMap<String, u64>> {
    let mut cells = HashMap::new();
    let mut total = None;
    for line in block.lines() {
        if let Some(count) = line.trim().strip_prefix("Number of cells:") {
            let count = count
                .trim()
                .parse::<u64>()
                .map_err(|_| invalid("malformed cell total"))?;
            if total.replace(count).is_some() {
                return Err(invalid("duplicate cell total"));
            }
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        let Some(count) = fields.first().and_then(|field| field.parse::<u64>().ok()) else {
            continue;
        };
        if matches!(fields.len(), 2 | 3) && fields.last() == Some(&"cells") {
            if total.replace(count).is_some() {
                return Err(invalid("duplicate cell total"));
            }
        } else if total.is_some() && matches!(fields.len(), 2 | 3) {
            let name = *fields.last().unwrap();
            if name == "wires" || name == "bits" {
                continue;
            }
            if cells.insert(name.into(), count).is_some() {
                return Err(invalid("duplicate cell row"));
            }
        }
    }
    // Current stat output omits the entire cells section for an empty design.
    let empty_table = block.contains("Local Count, excluding submodules.")
        && block.lines().all(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            fields.first().and_then(|v| v.parse::<u64>().ok()).is_none()
                || fields.len() != 2
                || matches!(fields[1], "wires" | "ports" | "memories" | "processes")
        });
    let expected = total
        .or(empty_table.then_some(0))
        .ok_or_else(|| invalid("missing cell total"))?;
    let sum = cells
        .values()
        .try_fold(0_u64, |sum, count| sum.checked_add(*count))
        .ok_or_else(|| invalid("cell count overflow"))?;
    if sum != expected {
        return Err(invalid("cell table does not match its total"));
    }
    Ok(cells)
}
