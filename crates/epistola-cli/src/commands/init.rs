use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use epistola_engine::init::init_collection;

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Directory to initialize (created if it doesn't exist)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Collection name (defaults to the directory name)
    #[arg(long)]
    pub name: Option<String>,

    #[arg(long)]
    pub description: Option<String>,
}

pub fn run(args: InitArgs) -> Result<()> {
    let outcome = init_collection(
        &args.path,
        args.name.as_deref(),
        args.description.as_deref(),
    )
    .with_context(|| {
        format!(
            "failed to initialize collection at '{}'",
            args.path.display()
        )
    })?;

    println!(
        "Initialized collection '{}' at {}",
        outcome.name,
        outcome.path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_format::CollectionManifest;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn run_creates_a_manifest_and_gitignore() {
        let dir = tempdir().unwrap();
        run(InitArgs {
            path: dir.path().to_path_buf(),
            name: Some("My API".to_string()),
            description: None,
        })
        .unwrap();

        let manifest = CollectionManifest::load(&dir.path().join("epistola.toml")).unwrap();
        assert_eq!(manifest.name, "My API");

        let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains("*.secrets.toml"));
        assert!(gitignore.contains(".epistola/"));
    }

    #[test]
    fn run_creates_missing_directories() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("collection");
        run(InitArgs {
            path: path.clone(),
            name: Some("n".to_string()),
            description: None,
        })
        .unwrap();
        assert!(path.join("epistola.toml").is_file());
    }

    #[test]
    fn run_defaults_the_name_to_the_directory_name_when_omitted() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("my-collection");
        run(InitArgs {
            path: path.clone(),
            name: None,
            description: None,
        })
        .unwrap();

        let manifest = CollectionManifest::load(&path.join("epistola.toml")).unwrap();
        assert_eq!(manifest.name, "my-collection");
    }
}
