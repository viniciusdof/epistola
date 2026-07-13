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
    #[serde(default, skip_serializing_if = "ClientSpec::is_default")]
    pub client: ClientSpec,
    /// Environment `run`/`request show --resolved` fall back to when
    /// `--env` isn't given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_environment: Option<String>,
    /// Collection-wide default for whether executed requests are appended
    /// to the local history log. `None` (the key absent) means "default to
    /// enabled." Same nearest-explicit-wins chain as `folder.toml`'s and
    /// `.req.toml`'s `history` fields — see
    /// [`crate::UnresolvedRequest::history_enabled`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<bool>,
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
            client: ClientSpec::default(),
            default_environment: None,
            history: None,
        };
        write_toml_file(path, &manifest)
    }

    /// Overwrites `path` with `self`'s current contents.
    pub fn save(&self, path: &Path) -> Result<(), FormatError> {
        write_toml_file(path, self)
    }
}

/// `[client]` in `epistola.toml` — collection-wide defaults for request
/// execution behavior. All fields are optional; a CLI flag overrides the
/// matching field (see `epistola-cli`'s `ClientArgs::resolve`).
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ClientSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    /// Max redirects to follow; `Some(0)` disables following redirects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    /// Skip TLS certificate validation for every request in this collection
    /// unless overridden. Dangerous.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub insecure: bool,
    /// Path (relative to the collection root) to a combined cert+key PEM
    /// file, for mutual-TLS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_cert: Option<String>,
}

impl ClientSpec {
    fn is_default(&self) -> bool {
        *self == ClientSpec::default()
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
        assert_eq!(manifest.client, ClientSpec::default());
    }

    #[test]
    fn parses_a_client_table() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("epistola.toml");
        std::fs::write(
            &path,
            "name = \"n\"\n\n[client]\ntimeout_secs = 30\nmax_redirects = 0\nproxy = \"http://proxy.local:8080\"\ninsecure = true\nclient_cert = \"client.pem\"\n",
        )
        .unwrap();
        let manifest = CollectionManifest::load(&path).unwrap();
        assert_eq!(manifest.client.timeout_secs, Some(30));
        assert_eq!(manifest.client.max_redirects, Some(0));
        assert_eq!(
            manifest.client.proxy.as_deref(),
            Some("http://proxy.local:8080")
        );
        assert!(manifest.client.insecure);
        assert_eq!(manifest.client.client_cert.as_deref(), Some("client.pem"));
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

    #[test]
    fn create_defaults_default_environment_to_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("epistola.toml");
        CollectionManifest::create(&path, "n", None).unwrap();
        assert_eq!(
            CollectionManifest::load(&path).unwrap().default_environment,
            None
        );
    }

    #[test]
    fn create_defaults_history_to_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("epistola.toml");
        CollectionManifest::create(&path, "n", None).unwrap();
        assert_eq!(CollectionManifest::load(&path).unwrap().history, None);
    }

    #[test]
    fn parses_an_explicit_history_false() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("epistola.toml");
        std::fs::write(&path, "name = \"n\"\nhistory = false\n").unwrap();
        let manifest = CollectionManifest::load(&path).unwrap();
        assert_eq!(manifest.history, Some(false));
    }

    #[test]
    fn save_overwrites_the_manifest_in_place() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("epistola.toml");
        CollectionManifest::create(&path, "n", None).unwrap();

        let mut manifest = CollectionManifest::load(&path).unwrap();
        manifest.default_environment = Some("dev".to_string());
        manifest.save(&path).unwrap();

        assert_eq!(
            CollectionManifest::load(&path)
                .unwrap()
                .default_environment
                .as_deref(),
            Some("dev")
        );
    }
}
