use anyhow::Result;
use clap::{Args, CommandFactory};
use clap_complete::{generate, Shell};

use crate::app::App;

#[derive(Args, Debug)]
pub struct CompletionsArgs {
    /// Shell to generate a completion script for
    pub shell: Shell,
}

pub fn run(args: CompletionsArgs) -> Result<()> {
    let mut cmd = App::command();
    let name = cmd.get_name().to_string();
    generate(args.shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn run_succeeds_for_every_supported_shell() {
        for shell in [
            Shell::Bash,
            Shell::Zsh,
            Shell::Fish,
            Shell::PowerShell,
            Shell::Elvish,
        ] {
            run(CompletionsArgs { shell }).unwrap();
        }
    }
}
