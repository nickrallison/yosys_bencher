use std::{hash::Hash, path::PathBuf};

use getset::{CloneGetters, CopyGetters, Getters, MutGetters, Setters, WithSetters};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::src_manager::git::history::RevisionSelection;

mod error;

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

#[derive(Copy, Clone, Debug, Getters)]
pub struct Version {
    #[get]
    minor: u32,
    #[get]
    major: u32,
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

    fn name(&self) -> &str;
    fn version(&self) -> Version;

    fn subset(&self, other: &Self) -> bool {
        todo!("implement subset")
    }
    fn superset(&self, other: &Self) -> bool {
        todo!("implement superset")
    }
}

pub struct RunContext;

// A single test instance
// need to implement Eq and Hash
pub trait Benchmark: Serialize {
    // Add traits to input, like a custom EQ for versioning
    type Input: Eq + Hash;
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

pub trait BenchmarkOut: Serialize + DeserializeOwned {}
