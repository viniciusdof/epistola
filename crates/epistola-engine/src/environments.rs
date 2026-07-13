use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use epistola_format::{
    create_environment, delete_environment, load_environment, rename_environment,
    set_environment_variable, LoadedCollection,
};

use crate::discovery::discover_collection;
use crate::error::EngineError;

fn manifest_path(root: &Path) -> PathBuf {
    root.join("epistola.toml")
}

/// Clears `default_environment` in the manifest if it currently points at
/// `name`, so deleting/renaming an environment never leaves a dangling
/// default behind.
fn clear_default_if(collection: &LoadedCollection, name: &str) -> Result<(), EngineError> {
    if collection.manifest.default_environment.as_deref() != Some(name) {
        return Ok(());
    }
    let mut manifest = collection.manifest.clone();
    manifest.default_environment = None;
    manifest.save(&manifest_path(&collection.root))?;
    Ok(())
}

pub fn new_environment(cwd: &Path, name: &str) -> Result<PathBuf, EngineError> {
    let collection = discover_collection(cwd)?;
    Ok(create_environment(&collection.root, name)?)
}

pub fn set_variable(
    cwd: &Path,
    name: &str,
    key: &str,
    value: &str,
    secret: bool,
) -> Result<PathBuf, EngineError> {
    let collection = discover_collection(cwd)?;
    Ok(set_environment_variable(
        &collection.root,
        name,
        key,
        value,
        secret,
    )?)
}

/// Enumerates environment names by scanning `environments/*.toml` and
/// `*.secrets.toml`, deduplicating the two files for the same environment.
pub fn list_environment_names(cwd: &Path) -> Result<BTreeSet<String>, EngineError> {
    let collection = discover_collection(cwd)?;
    let environments_dir = collection.root.join("environments");

    let mut names = BTreeSet::new();
    if environments_dir.is_dir() {
        for entry in std::fs::read_dir(&environments_dir).map_err(|source| EngineError::Io {
            path: environments_dir.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| EngineError::Io {
                path: environments_dir.clone(),
                source,
            })?;
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            // Longest suffix first, so dev.toml/dev.secrets.toml dedupe to "dev".
            let base = file_name
                .strip_suffix(".secrets.toml")
                .or_else(|| file_name.strip_suffix(".toml"));
            if let Some(base) = base {
                names.insert(base.to_string());
            }
        }
    }

    Ok(names)
}

pub fn delete(cwd: &Path, name: &str) -> Result<(), EngineError> {
    let collection = discover_collection(cwd)?;
    delete_environment(&collection.root, name)?;
    clear_default_if(&collection, name)
}

pub fn rename(cwd: &Path, name: &str, new_name: &str) -> Result<(), EngineError> {
    let collection = discover_collection(cwd)?;
    rename_environment(&collection.root, name, new_name)?;

    if collection.manifest.default_environment.as_deref() == Some(name) {
        let mut manifest = collection.manifest.clone();
        manifest.default_environment = Some(new_name.to_string());
        manifest.save(&manifest_path(&collection.root))?;
    }
    Ok(())
}

pub fn get_default(cwd: &Path) -> Result<Option<String>, EngineError> {
    let collection = discover_collection(cwd)?;
    Ok(collection.manifest.default_environment)
}

pub fn set_default(cwd: &Path, name: &str) -> Result<(), EngineError> {
    let collection = discover_collection(cwd)?;
    load_environment(&collection.root, name)?;

    let mut manifest = collection.manifest.clone();
    manifest.default_environment = Some(name.to_string());
    manifest.save(&manifest_path(&collection.root))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_format::CollectionManifest;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn new_environment_errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        assert!(new_environment(dir.path(), "dev").is_err());
    }

    #[test]
    fn new_environment_creates_an_empty_environment() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        new_environment(dir.path(), "dev").unwrap();

        assert!(dir.path().join("environments/dev.toml").is_file());
    }

    #[test]
    fn set_variable_creates_the_public_file_by_default() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        set_variable(dir.path(), "dev", "k", "v", false).unwrap();

        assert!(dir.path().join("environments/dev.toml").is_file());
        assert!(!dir.path().join("environments/dev.secrets.toml").is_file());
    }

    #[test]
    fn set_variable_with_secret_writes_the_sidecar() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        set_variable(dir.path(), "dev", "k", "v", true).unwrap();

        assert!(dir.path().join("environments/dev.secrets.toml").is_file());
        assert!(!dir.path().join("environments/dev.toml").is_file());
    }

    #[test]
    fn list_environment_names_errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        assert!(list_environment_names(dir.path()).is_err());
    }

    #[test]
    fn list_environment_names_deduplicates_public_and_secrets_files() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        set_variable(dir.path(), "dev", "k", "v", false).unwrap();
        set_variable(dir.path(), "dev", "k", "v", true).unwrap();
        set_variable(dir.path(), "prod", "k", "v", false).unwrap();

        let names = list_environment_names(dir.path()).unwrap();
        assert_eq!(
            names,
            BTreeSet::from(["dev".to_string(), "prod".to_string()])
        );
    }

    #[test]
    fn list_environment_names_works_when_no_environments_directory_exists_yet() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        assert!(list_environment_names(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn delete_removes_the_environment() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        new_environment(dir.path(), "dev").unwrap();

        delete(dir.path(), "dev").unwrap();

        assert!(!dir.path().join("environments/dev.toml").is_file());
    }

    #[test]
    fn delete_errors_when_the_environment_is_missing() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        assert!(delete(dir.path(), "dev").is_err());
    }

    #[test]
    fn delete_clears_the_default_environment_if_it_matches() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        new_environment(dir.path(), "dev").unwrap();
        set_default(dir.path(), "dev").unwrap();

        delete(dir.path(), "dev").unwrap();

        let manifest = CollectionManifest::load(&dir.path().join("epistola.toml")).unwrap();
        assert_eq!(manifest.default_environment, None);
    }

    #[test]
    fn rename_moves_the_environment_and_preserves_variables() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        set_variable(dir.path(), "dev", "k", "v", false).unwrap();

        rename(dir.path(), "dev", "staging").unwrap();

        assert!(!dir.path().join("environments/dev.toml").is_file());
        assert!(dir.path().join("environments/staging.toml").is_file());
    }

    #[test]
    fn rename_updates_the_default_environment_if_it_matches() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        new_environment(dir.path(), "dev").unwrap();
        set_default(dir.path(), "dev").unwrap();

        rename(dir.path(), "dev", "staging").unwrap();

        let manifest = CollectionManifest::load(&dir.path().join("epistola.toml")).unwrap();
        assert_eq!(manifest.default_environment.as_deref(), Some("staging"));
    }

    #[test]
    fn get_default_is_none_when_unset() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        assert_eq!(get_default(dir.path()).unwrap(), None);
    }

    #[test]
    fn set_default_errors_when_the_named_environment_does_not_exist() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        assert!(set_default(dir.path(), "dev").is_err());
    }

    #[test]
    fn set_default_sets_and_persists_the_default_environment() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        new_environment(dir.path(), "dev").unwrap();

        set_default(dir.path(), "dev").unwrap();

        let manifest = CollectionManifest::load(&dir.path().join("epistola.toml")).unwrap();
        assert_eq!(manifest.default_environment.as_deref(), Some("dev"));
    }
}
