use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::Args;

#[derive(Args, Debug)]
pub struct OpenArgs {
    /// Directory to open in the GPUI desktop client
    #[arg(default_value = ".")]
    pub path: PathBuf,
}

pub fn run(args: OpenArgs) -> Result<()> {
    let target = args
        .path
        .canonicalize()
        .with_context(|| format!("'{}' does not exist", args.path.display()))?;
    let gui_binary = locate_gui_binary()?;

    Command::new(gui_binary)
        .current_dir(&target)
        .spawn()
        .context("failed to launch epistola_gui")?;
    Ok(())
}

fn locate_gui_binary() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("couldn't locate the running epistola binary")?;
    let dir = exe
        .parent()
        .context("the running binary has no parent directory")?;
    let candidate = dir.join(gui_binary_name());
    if candidate.is_file() {
        Ok(candidate)
    } else {
        bail!(
            "no epistola_gui binary found next to this CLI ({}) — this build may not include the GUI",
            candidate.display()
        );
    }
}

fn gui_binary_name() -> String {
    format!("epistola_gui{}", std::env::consts::EXE_SUFFIX)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn run_fails_with_a_clear_message_when_the_path_does_not_exist() {
        let err = run(OpenArgs {
            path: PathBuf::from("/does/not/exist/anywhere"),
        })
        .unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn locate_gui_binary_fails_with_a_clear_message_in_the_test_binary() {
        // The test harness binary isn't named `epistola_gui`, so this should
        // always fail here — asserting the error message stays actionable.
        let err = locate_gui_binary().unwrap_err();
        assert!(err.to_string().contains("epistola_gui"));
    }

    #[test]
    fn gui_binary_name_follows_the_platform_convention() {
        assert!(gui_binary_name().starts_with("epistola_gui"));
    }
}
