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

    append_gitignore_entries(&args.path, &["*.secrets.toml", ".epistola/"])?;

    println!("Initialized collection '{name}' at {}", args.path.display());
    Ok(())
}

fn default_collection_name(path: &std::path::Path) -> String {
    path.canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "collection".to_string())
}

/// Appends any of `entries` not already present to `.gitignore`, creating
/// it if needed. Used for `*.secrets.toml` (environment/global secrets)
/// and `.epistola/` (the local history log) — anything generated on this
/// machine that shouldn't be committed.
fn append_gitignore_entries(collection_root: &std::path::Path, entries: &[&str]) -> Result<()> {
    let path = collection_root.join(".gitignore");
    let mut content = std::fs::read_to_string(&path).unwrap_or_default();

    let mut changed = false;
    for entry in entries {
        if content.lines().any(|line| line.trim() == *entry) {
            continue;
        }
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(entry);
        content.push('\n');
        changed = true;
    }

    if changed {
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write '{}'", path.display()))?;
    }
    Ok(())
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
    fn append_gitignore_entries_does_not_duplicate_an_existing_entry() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "*.secrets.toml\n").unwrap();
        append_gitignore_entries(dir.path(), &["*.secrets.toml"]).unwrap();
        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(content.matches("*.secrets.toml").count(), 1);
    }

    #[test]
    fn append_gitignore_entries_preserves_existing_content() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "/target\n").unwrap();
        append_gitignore_entries(dir.path(), &["*.secrets.toml"]).unwrap();
        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(content.contains("/target"));
        assert!(content.contains("*.secrets.toml"));
    }

    #[test]
    fn append_gitignore_entries_does_not_duplicate_any_entry_across_multiple_calls() {
        let dir = tempdir().unwrap();
        append_gitignore_entries(dir.path(), &["*.secrets.toml", ".epistola/"]).unwrap();
        append_gitignore_entries(dir.path(), &["*.secrets.toml", ".epistola/"]).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(content.matches("*.secrets.toml").count(), 1);
        assert_eq!(content.matches(".epistola/").count(), 1);
    }
}
