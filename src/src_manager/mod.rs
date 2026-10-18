pub mod build;
pub mod error;
pub mod git;

pub use error::SourceManagerError;

pub trait SourceManager: Sized {
    type Revision;

    fn apply_revision(&self, revision: Self::Revision) -> Result<(), SourceManagerError>;
    fn initialize() -> Result<Self, SourceManagerError>;
}
