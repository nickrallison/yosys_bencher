#[macro_use]
extern crate getset;

pub mod analysis;
pub mod benchmarks;
pub mod error;
pub mod experiment;
pub mod metrics;
pub mod src_manager;
pub mod store;
pub mod tools;

pub use error::Error;

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
pub(crate) mod test_support;
