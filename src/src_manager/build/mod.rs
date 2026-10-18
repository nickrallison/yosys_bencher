mod cmake;
pub mod error;
// mod yosys;

pub use error::BuildError;
// pub use yosys::{Yosys, YosysBuilder};

use super::git::history::Commit;

pub trait Builder {
    type Tool;

    fn build(&mut self, commit: &Commit) -> Result<Self::Tool, BuildError>;
}
