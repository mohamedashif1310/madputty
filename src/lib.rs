//! madputty library crate — exposes internals for tests and benchmarks.
//!
//! The binary entry point remains in main.rs. This lib target lets
//! integration tests and criterion benchmarks import madputty modules
//! without going through the binary.

pub mod ai;
pub mod cli;
pub mod errors;
pub mod io;
pub mod serial_config;
pub mod serial_port_trait;
pub mod theme;
pub mod ui;
