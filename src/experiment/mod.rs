use std::{hash::Hash, path::PathBuf};

use getset::Getters;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::experiment::perf::PerfProfiler;
use crate::src_manager::build::CcacheCMakeBuilder;
use crate::src_manager::git::history::RevisionSelection;
use crate::store::Store;

mod error;
mod perf;

pub use error::ExperimentError;

#[derive(Clone, Debug, Getters)]
pub struct Experiment<S: Suite> {
    #[get]
    revs: RevisionSelection,
    #[get]
    suite: S,
    #[get]
    output_dir: PathBuf,
}

impl<S: Suite> Experiment<S> {
    pub fn run<B, D>(
        experiment: &Experiment<S>,
        builder: &mut CcacheCMakeBuilder,
        store: &mut Store,
        options: &RunOptions,
    ) -> Result<(), ExperimentError> {
        todo!("Stub out Experiment::run")
    }
}

#[derive(Debug, Default, Getters)]
pub struct RunOptions {
    /// Collect diagnostic perf recordings after measured Yosys phases.
    #[get]
    perf: Option<PerfProfiler>,
}

/// Isolated workspace and process/cache helpers for one comparison attempt.
///
/// Use fresh temporary paths for repeated invocations and remove temporary
/// parser inputs after use. Experiment orchestration places retained workspaces
/// under its configured output layout, timestamp, and benchmark/commit directory.
#[derive(Debug, Getters)]
pub struct RunContext<'a, C> {
    #[get]
    workspace: PathBuf,
    #[get]
    artifacts: ArtifactPolicy,
    /// Only reuse reference timings within this compatible measurement session.
    /// `None` forces fresh reference measurements. Experiments set their own ID.
    #[get]
    reference_scope: Option<String>,
    /// Optional diagnostic recordings, outside the stored timing samples.
    #[get]
    perf: Option<&'a PerfProfiler>,
    #[get]
    cache: &'a mut C,
    #[get]
    processes: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Getters)]
pub struct ArtifactPolicy {
    #[get]
    logs: bool,
    #[get]
    scripts: bool,
    #[get]
    netlists: bool,
}

#[derive(Copy, Clone, Debug, Getters)]
pub struct Version {
    #[get]
    minor: u32,
    #[get]
    major: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum CmpSuite {
    /// a.cmp(b) = Subset -> b is a subset of a
    Subset,
    Eq,
    Not,
}

/// An unordered Set of Benchmarks
pub trait Suite {
    type Tool;
    type Benchmark: Benchmark;

    fn input(
        &self,
        tool: &Self::Tool,
    ) -> Result<<Self::Benchmark as Benchmark>::Input, ExperimentError>;
    fn benchmarks(&self) -> &[Self::Benchmark];

    fn get_version(&self) -> &Version;
    fn inc_maj_version(&mut self);
    fn inc_min_version(&mut self);
    fn check_bump_version(&mut self, store: &Store) {
        let prev_stored_suite = todo!("Get prev stored suite from store");

        match self.cmp(prev_stored_suite) {
            CmpSuite::Subset => {
                self.inc_min_version();
            }
            CmpSuite::Eq => {
                // nothing
            }
            CmpSuite::Not => {
                todo!("Confirm with user, if validated, bump major version");
                self.inc_maj_version();
            }
        }
    }

    fn name(&self) -> &str;
    fn version(&self) -> Version;

    fn cmp(&self, other: &Self) -> CmpSuite {
        todo!("implement cmp")
    }
}

// A single test instance
// need to implement Eq and Hash
pub trait Benchmark: Serialize {
    // Add traits to input, like a custom EQ for versioning
    type Input: BenchmarkIn;
    // are there any constraints on the output?
    // I know I want a frame like output
    type Output: BenchmarkOut;

    type Suite: Suite<Benchmark = Self>;

    fn run(
        &self,
        input: &Self::Input,
        context: &mut RunContext,
    ) -> Result<Self::Output, ExperimentError>;
}

pub trait BenchmarkIn: Eq + Hash + Serialize + DeserializeOwned {
    // Might want to be able to filter by itemtype later
}

pub trait BenchmarkOut: Serialize + DeserializeOwned {
    // which items are random vs deterministic
}
