use std::path::{Path, PathBuf};

use epistola_format::CollectionManifest;

use crate::error::EngineError;

pub struct InitOutcome {
    pub name: String,
    pub path: PathBuf,
}

/// Scaffolds a new collection: creates `path` if needed, writes
/// `epistola.toml`, and gitignores the local, machine-generated paths
/// (`*.secrets.toml`, `.epistola/`).
pub fn init_collection(
    path: &Path,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<InitOutcome, EngineError> {
    std::fs::create_dir_all(path).map_err(|source| EngineError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let name = name
        .map(str::to_string)
        .unwrap_or_else(|| default_collection_name(path));

    let manifest_path = path.join("epistola.toml");
    CollectionManifest::create(&manifest_path, &name, description)?;

    append_gitignore_entries(path, &["*.secrets.toml", ".epistola/"])?;

    Ok(InitOutcome {
        name,
        path: path.to_path_buf(),
    })
}

fn default_collection_name(path: &Path) -> String {
    path.canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "collection".to_string())
}

/// Appends any of `entries` not already present to `.gitignore`, creating
/// it if needed. Used for `*.secrets.toml` (environment/global secrets)
/// and `.epistola/` (the local history log) — anything generated on this
/// machine that shouldn't be committed.
fn append_gitignore_entries(collection_root: &Path, entries: &[&str]) -> Result<(), EngineError> {
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
        std::fs::write(&path, content).map_err(|source| EngineError::Io { path, source })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn init_collection_creates_a_manifest_and_gitignore() {
        let dir = tempdir().unwrap();
        let outcome = init_collection(dir.path(), Some("My API"), None).unwrap();
        assert_eq!(outcome.name, "My API");

        let manifest = CollectionManifest::load(&dir.path().join("epistola.toml")).unwrap();
        assert_eq!(manifest.name, "My API");

        let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains("*.secrets.toml"));
        assert!(gitignore.contains(".epistola/"));
    }

    #[test]
    fn init_collection_creates_missing_directories() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("collection");
        init_collection(&path, Some("n"), None).unwrap();
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
