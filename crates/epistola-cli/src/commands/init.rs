use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use epistola_format::CollectionManifest;

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
    std::fs::create_dir_all(&args.path)
        .with_context(|| format!("failed to create '{}'", args.path.display()))?;

    let name = args
        .name
        .clone()
        .unwrap_or_else(|| default_collection_name(&args.path));

    let manifest_path = args.path.join("epistola.toml");
    CollectionManifest::create(&manifest_path, &name, args.description.as_deref())
        .with_context(|| format!("failed to create '{}'", manifest_path.display()))?;

    append_secrets_gitignore(&args.path)?;

    println!("Initialized collection '{name}' at {}", args.path.display());
    Ok(())
}

fn default_collection_name(path: &std::path::Path) -> String {
    path.canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "collection".to_string())
}

/// Appends `*.secrets.toml` to `.gitignore`, creating it if needed.
fn append_secrets_gitignore(collection_root: &std::path::Path) -> Result<()> {
    let entry = "*.secrets.toml";
    let path = collection_root.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    if existing.lines().any(|line| line.trim() == entry) {
        return Ok(());
    }

    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(entry);
    content.push('\n');

    std::fs::write(&path, content).with_context(|| format!("failed to write '{}'", path.display()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

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
    fn append_secrets_gitignore_does_not_duplicate_an_existing_entry() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "*.secrets.toml\n").unwrap();
        append_secrets_gitignore(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(content.matches("*.secrets.toml").count(), 1);
    }

    #[test]
    fn append_secrets_gitignore_preserves_existing_content() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "/target\n").unwrap();
        append_secrets_gitignore(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(content.contains("/target"));
        assert!(content.contains("*.secrets.toml"));
    }
}
