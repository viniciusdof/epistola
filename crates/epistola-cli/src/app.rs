use clap::{Parser, Subcommand};

use crate::commands::{env, init, request, run};

/// A Rust-native HTTP client, built for the terminal.
#[derive(Parser, Debug)]
#[command(name = "epistola", version, about)]
pub struct App {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Scaffold a new collection (epistola.toml + environments/ + .gitignore)
    Init(init::InitArgs),
    /// Manage saved requests in the current collection
    Request(request::RequestArgs),
    /// Manage environments in the current collection
    Env(env::EnvArgs),
    /// Resolve and execute a saved request
    Run(run::RunArgs),
    /// Falls through to an ad-hoc request: `epistola GET <url> ...`
    #[command(external_subcommand)]
    Send(Vec<String>),
}
