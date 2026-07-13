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
}
