//! Shared values for native clients and the browser. No database or tool dependencies.
#[macro_use]
extern crate getset;
pub mod analysis;
pub mod api;
pub mod experiment;
pub mod metrics;
pub mod store;
pub use store::Version;
