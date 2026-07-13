mod app;
mod cli;
mod client_config;
mod commands;
mod errors;
mod history;
mod output;

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use app::{App, Command};
use errors::CliError;

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
        Command::Completions(args) => commands::completions::run(args),
        Command::Send(args) => commands::send::run(args, &cwd).await,
    }
}

/// Maps an error to a process exit code so scripts can distinguish failure
/// classes. `2` is reserved by clap for argument-parsing errors (it exits
/// before `run` is ever called).
fn exit_code_for(err: &anyhow::Error) -> u8 {
    if err.downcast_ref::<CliError>().is_some() {
        return 5;
    }
    if err.downcast_ref::<epistola_core::ExecutorError>().is_some() {
        return 4;
    }
    if err.downcast_ref::<epistola_format::FormatError>().is_some() {
        return 3;
    }
    1
}
