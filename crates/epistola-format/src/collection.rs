use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::FormatError;
use crate::toml_file::{read_toml_file, write_toml_file};

/// `epistola.toml` — a collection's manifest, at the collection root.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CollectionManifest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
}

impl CollectionManifest {
    pub fn load(path: &Path) -> Result<Self, FormatError> {
        read_toml_file(path)
    }

    /// Scaffolds a new manifest; errors if `path` already exists.
    pub fn create(path: &Path, name: &str, description: Option<&str>) -> Result<(), FormatError> {
        if path.is_file() {
            return Err(FormatError::AlreadyExists {
                path: path.to_path_buf(),
            });
        }
        let manifest = CollectionManifest {
            name: name.to_string(),
            description: description.map(str::to_string),
            variables: BTreeMap::new(),
        };
        write_toml_file(path, &manifest)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn parses_a_minimal_manifest_with_name_only() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("epistola.toml");
        std::fs::write(&path, "name = \"My collection\"\n").unwrap();
        let manifest = CollectionManifest::load(&path).unwrap();
        assert_eq!(manifest.name, "My collection");
        assert!(manifest.variables.is_empty());
    }

    #[test]
    fn parses_description_and_variables() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("epistola.toml");
        std::fs::write(
            &path,
            "name = \"n\"\ndescription = \"d\"\n\n[variables]\napi_version = \"v2\"\n",
        )
        .unwrap();
        let manifest = CollectionManifest::load(&path).unwrap();
        assert_eq!(manifest.description.as_deref(), Some("d"));
        assert_eq!(
            manifest.variables.get("api_version").map(String::as_str),
            Some("v2")
        );
    }

    #[test]
    fn load_errors_when_the_file_is_missing() {
        let dir = tempdir().unwrap();
        let err = CollectionManifest::load(&dir.path().join("epistola.toml")).unwrap_err();
        assert!(matches!(err, FormatError::Io { .. }));
    }

    #[test]
    fn create_writes_a_loadable_manifest() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("epistola.toml");
        CollectionManifest::create(&path, "My API", Some("desc")).unwrap();

        let manifest = CollectionManifest::load(&path).unwrap();
        assert_eq!(manifest.name, "My API");
        assert_eq!(manifest.description.as_deref(), Some("desc"));
    }

    #[test]
    fn create_refuses_to_overwrite_an_existing_manifest() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("epistola.toml");
        CollectionManifest::create(&path, "First", None).unwrap();

        let err = CollectionManifest::create(&path, "Second", None).unwrap_err();
        assert!(matches!(err, FormatError::AlreadyExists { .. }));
        // the original file must be untouched
        assert_eq!(CollectionManifest::load(&path).unwrap().name, "First");
    }
}
