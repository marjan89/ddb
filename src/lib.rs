//! Public-facing modules shared with integration tests.
//!
//! `ddb` is primarily a binary crate; this library exists so that
//! `tests/*.rs` can call into the parsers and other low-level helpers
//! without going through the CLI.

pub mod agent_yaml;
