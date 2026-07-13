use std::path::Path;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use epistola_engine::folder::init_folder;

#[derive(Args, Debug)]
pub struct FolderArgs {
    #[command(subcommand)]
    pub action: FolderAction,
}

#[derive(Subcommand, Debug)]
pub enum FolderAction {
    /// Scaffold an empty `folder.toml` — headers/auth every request under
    /// this directory inherits unless it opts out. See
    /// `request show --resolved` to preview the merged result.
    Init(InitArgs),
}

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Directory (relative to the collection root) to scaffold; defaults
    /// to the collection root itself
    #[arg(default_value = "")]
    pub dir: String,
}

pub fn run(args: FolderArgs, cwd: &Path) -> Result<()> {
    match args.action {
        FolderAction::Init(a) => init(a, cwd),
    }
}

fn init(args: InitArgs, cwd: &Path) -> Result<()> {
    let path = init_folder(cwd, &args.dir)
        .with_context(|| format!("failed to create folder.toml under '{}'", args.dir))?;
    println!("Created {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn init_creates_a_folder_toml_at_the_collection_root_by_default() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("epistola.toml"), "name = \"n\"\n").unwrap();

        init(InitArgs { dir: String::new() }, dir.path()).unwrap();

        assert!(dir.path().join("folder.toml").is_file());
    }

    #[test]
    fn init_creates_a_folder_toml_in_a_nested_directory() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("epistola.toml"), "name = \"n\"\n").unwrap();
        std::fs::create_dir_all(dir.path().join("auth")).unwrap();

        init(
            InitArgs {
                dir: "auth".to_string(),
            },
            dir.path(),
        )
        .unwrap();

        assert!(dir.path().join("auth").join("folder.toml").is_file());
    }

    #[test]
    fn init_refuses_to_overwrite_an_existing_folder_toml() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("epistola.toml"), "name = \"n\"\n").unwrap();
        init(InitArgs { dir: String::new() }, dir.path()).unwrap();

        assert!(init(InitArgs { dir: String::new() }, dir.path()).is_err());
    }
}
