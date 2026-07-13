//! Orchestration for epistola's use cases — resolving and executing saved or
//! ad-hoc requests, managing environments/folders/history — built on
//! `epistola-format` (parsing) and `epistola-http` (transport). `epistola-cli`
//! and a future GUI both consume this crate instead of duplicating this
//! logic; all public functions return [`EngineError`], not `anyhow::Result`,
//! so a caller can match on outcomes without downcasting.

pub mod adhoc;
pub mod client;
pub mod discovery;
pub mod environments;
mod error;
pub mod folder;
pub mod history;
pub mod init;
pub mod output;
pub mod recent;
pub mod requests;
pub mod resolve;
pub mod run;

pub use error::EngineError;
