use std::path::{Path, PathBuf};

use epistola_format::FolderManifest;

use crate::discovery::discover_collection;
use crate::error::EngineError;

/// Scaffolds an empty `folder.toml` at `dir` (relative to the collection
/// root; empty string means the collection root itself).
pub fn init_folder(cwd: &Path, dir: &str) -> Result<PathBuf, EngineError> {
    let collection = discover_collection(cwd)?;

    let target_dir = if dir.is_empty() {
        collection.root.clone()
    } else {
        collection.root.join(dir)
    };
    let path = target_dir.join("folder.toml");

    FolderManifest::create(&path)?;
    Ok(path)
}

/// Recursively finds every directory under `root` containing a `folder.toml`,
/// skipping hidden directories like [`crate::requests::find_request_files`].
pub fn find_folder_manifest_dirs(root: &Path) -> Result<Vec<PathBuf>, EngineError> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if dir.join("folder.toml").is_file() {
            found.push(dir.clone());
        }
        let entries = std::fs::read_dir(&dir).map_err(|source| EngineError::Io {
            path: dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| EngineError::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            if path.is_dir() {
                let is_hidden = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'));
                if !is_hidden {
                    stack.push(path);
                }
            }
        }
    }
    Ok(found)
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

        init_folder(dir.path(), "").unwrap();

        assert!(dir.path().join("folder.toml").is_file());
    }

    #[test]
    fn init_creates_a_folder_toml_in_a_nested_directory() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("epistola.toml"), "name = \"n\"\n").unwrap();
        std::fs::create_dir_all(dir.path().join("auth")).unwrap();

        init_folder(dir.path(), "auth").unwrap();

        assert!(dir.path().join("auth").join("folder.toml").is_file());
    }

    #[test]
    fn init_refuses_to_overwrite_an_existing_folder_toml() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("epistola.toml"), "name = \"n\"\n").unwrap();
        init_folder(dir.path(), "").unwrap();

        assert!(init_folder(dir.path(), "").is_err());
    }

    #[test]
    fn init_errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        assert!(init_folder(dir.path(), "").is_err());
    }

    #[test]
    fn find_folder_manifest_dirs_finds_the_root_and_nested_manifests() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("epistola.toml"), "name = \"n\"\n").unwrap();
        init_folder(dir.path(), "").unwrap();
        init_folder(dir.path(), "auth").unwrap();

        let mut found = find_folder_manifest_dirs(dir.path()).unwrap();
        found.sort();
        let mut expected = vec![dir.path().to_path_buf(), dir.path().join("auth")];
        expected.sort();
        assert_eq!(found, expected);
    }

    #[test]
    fn find_folder_manifest_dirs_skips_hidden_directories() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("epistola.toml"), "name = \"n\"\n").unwrap();
        let hidden = dir.path().join(".epistola");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join("folder.toml"), "").unwrap();

        let found = find_folder_manifest_dirs(dir.path()).unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn find_folder_manifest_dirs_is_empty_when_no_manifest_exists() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("epistola.toml"), "name = \"n\"\n").unwrap();

        let found = find_folder_manifest_dirs(dir.path()).unwrap();
        assert!(found.is_empty());
    }
}
