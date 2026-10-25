use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperStats {
    pub runtime: Duration,
    pub adds_subs: u64,
    pub shifts: u64,
    pub negations: u64,
    pub multipliers: u64,
    pub depth: u32,
}

crate::benchmark_metrics! {
    PaperStats {
        scalar runtime(Duration, "Paper process runtime", "s", true);
        scalar adds_subs(UInt, "Add/subtract operations", "operations", false);
        scalar shifts(UInt, "Shifts", "operations", false);
        scalar negations(UInt, "Negations", "operations", false);
        scalar multipliers(UInt, "Remaining multipliers", "operations", false);
        scalar depth(UInt, "Paper depth", "levels", false);
    }
}
