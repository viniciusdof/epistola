mod app;
mod cli;
mod client_config;
mod commands;
mod output;

use anyhow::{Context, Result};
use clap::Parser;

use app::{App, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let cwd = std::env::current_dir().context("failed to determine the current directory")?;

    match App::parse().command {
        Command::Init(args) => commands::init::run(args),
        Command::Request(args) => commands::request::run(args, &cwd),
        Command::Env(args) => commands::env::run(args, &cwd),
        Command::Run(args) => commands::run::run(args).await,
        Command::Send(args) => commands::send::run(args, &cwd).await,
    }
}
