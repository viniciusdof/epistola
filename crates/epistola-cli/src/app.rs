use clap::{Parser, Subcommand};

use crate::commands::{completions, env, folder, init, request, run};

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
    /// Manage folder-level header/auth inheritance (`folder.toml`)
    Folder(folder::FolderArgs),
    /// Resolve and execute a saved request
    Run(run::RunArgs),
    /// Generate a shell completion script, printed to stdout
    Completions(completions::CompletionsArgs),
    /// Falls through to an ad-hoc request: `epistola GET <url> ...`
    #[command(external_subcommand)]
    Send(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definition_is_well_formed() {
        <App as clap::CommandFactory>::command().debug_assert();
    }
}
