mod app;
mod cli;
mod client_config;
mod commands;
mod output;

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use epistola_engine::EngineError;

use app::{App, Command};

#[tokio::main]
async fn main() -> ExitCode {
    let result = run().await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err:?}");
            ExitCode::from(exit_code_for(&err))
        }
    }
}

async fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("failed to determine the current directory")?;

    match App::parse().command {
        Command::Init(args) => commands::init::run(args),
        Command::Request(args) => commands::request::run(args, &cwd),
        Command::Env(args) => commands::env::run(args, &cwd),
        Command::Folder(args) => commands::folder::run(args, &cwd),
        Command::Run(args) => commands::run::run(args).await,
        Command::History(args) => commands::history::run(args, &cwd),
        Command::Open(args) => commands::open::run(args),
        Command::Completions(args) => commands::completions::run(args),
        Command::Send(args) => commands::send::run(args, &cwd).await,
    }
}

/// Maps an error to a process exit code so scripts can distinguish failure
/// classes. `2` is reserved by clap for argument-parsing errors (it exits
/// before `run` is ever called). Every command path routes its
/// domain-level errors through `epistola_engine::EngineError`, so this is
/// the only downcast needed.
fn exit_code_for(err: &anyhow::Error) -> u8 {
    match err.downcast_ref::<EngineError>() {
        Some(EngineError::HttpStatusFailure(_) | EngineError::LintFailed(_)) => 5,
        Some(EngineError::Executor(_)) => 4,
        Some(EngineError::Format(_) | EngineError::NotInCollection { .. }) => 3,
        _ => 1,
    }
}
