use std::path::Path;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use epistola_format::{FolderManifest, LoadedCollection};

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
    let collection = LoadedCollection::discover_from(cwd).context(
        "not inside a collection (no epistola.toml found in this or any parent directory)",
    )?;

    let dir = if args.dir.is_empty() {
        collection.root.clone()
    } else {
        collection.root.join(&args.dir)
    };
    let path = dir.join("folder.toml");

    FolderManifest::create(&path)
        .with_context(|| format!("failed to create '{}'", path.display()))?;

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

        let err = init(InitArgs { dir: String::new() }, dir.path()).unwrap_err();
        assert!(err.to_string().contains("failed to create"));
    }
}
