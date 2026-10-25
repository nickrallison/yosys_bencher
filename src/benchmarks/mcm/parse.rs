use super::PaperStats;
use crate::experiment::ExperimentError;
use std::time::Duration;
type Result<T> = std::result::Result<T, ExperimentError>;
fn invalid(message: &str) -> ExperimentError {
    ExperimentError::Parse(message.into())
}

pub(super) fn paper(text: &str, elapsed: Duration) -> Result<PaperStats> {
    let mut cost = None;
    let mut depth = None;
    let mut multipliers = 0;
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("// Cost: ") {
            let fields: Vec<_> = rest.split_whitespace().collect();
            if fields.len() != 6
                || fields[1] != "adds/subtracts"
                || fields[3] != "shifts"
                || fields[5] != "negations"
            {
                return Err(invalid("malformed paper cost"));
            }
            let number = |index: usize| {
                fields[index]
                    .parse::<u64>()
                    .map_err(|_| invalid("malformed paper count"))
            };
            if cost.replace((number(0)?, number(2)?, number(4)?)).is_some() {
                return Err(invalid("duplicate paper cost"));
            }
        }
        if let Some(rest) = line.strip_prefix("// Depth: ") {
            let value = rest
                .parse::<u32>()
                .map_err(|_| invalid("malformed paper depth"))?;
            if depth.replace(value).is_some() {
                return Err(invalid("duplicate paper depth"));
            }
        }
        if let Some((target, expression)) = line.split_once('=')
            && target
                .trim()
                .strip_prefix('t')
                .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
            && expression.trim_start().starts_with("cmul(")
        {
            multipliers += 1;
        }
    }
    let (adds_subs, shifts, negations) = cost.ok_or_else(|| invalid("missing paper cost"))?;
    Ok(PaperStats {
        runtime: elapsed,
        adds_subs,
        shifts,
        negations,
        multipliers,
        depth: depth.ok_or_else(|| invalid("missing paper depth"))?,
    })
}
