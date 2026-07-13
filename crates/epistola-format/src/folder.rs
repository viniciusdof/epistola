use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::FormatError;
use crate::request_file::{AuthSpec, HeaderEntry};
use crate::toml_file::{read_toml_file, write_toml_file};

/// `folder.toml` — optional, at most one per directory (including the
/// collection root itself), sibling to `.req.toml` files. Provides
/// headers/auth that every request under that directory inherits unless it
/// opts out. See [`load_folder_chain`] and
/// [`crate::UnresolvedRequest::with_folder_inheritance`].
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FolderManifest {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<HeaderEntry>,
    /// `None` (the key absent) means "no opinion, keep asking my parent
    /// folder." `Some(AuthSpec::None)` (an explicit `type = "none"`) blocks
    /// inheritance from going any further up the tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthSpec>,
}

impl FolderManifest {
    pub fn load(path: &Path) -> Result<Self, FormatError> {
        read_toml_file(path)
    }

    /// Scaffolds an empty `folder.toml`; errors if `path` already exists.
    pub fn create(path: &Path) -> Result<(), FormatError> {
        if path.is_file() {
            return Err(FormatError::AlreadyExists {
                path: path.to_path_buf(),
            });
        }
        write_toml_file(path, &FolderManifest::default())
    }
}

/// Loads every `folder.toml` from `collection_root` down to (and including)
/// `request_dir`, root-to-leaf order. A directory without a `folder.toml`
/// is skipped, not an error — it's opt-in per directory. If `request_dir`
/// isn't under `collection_root`, only the root's own `folder.toml` (if
/// any) is considered.
pub fn load_folder_chain(
    collection_root: &Path,
    request_dir: &Path,
) -> Result<Vec<FolderManifest>, FormatError> {
    let mut dirs = vec![collection_root.to_path_buf()];
    if let Ok(rel) = request_dir.strip_prefix(collection_root) {
        let mut current = collection_root.to_path_buf();
        for component in rel.components() {
            current = current.join(component);
            dirs.push(current.clone());
        }
    }

    let mut chain = Vec::new();
    for dir in dirs {
        let path = dir.join("folder.toml");
        if path.is_file() {
            chain.push(FolderManifest::load(&path)?);
        }
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn create_writes_a_loadable_empty_manifest() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("folder.toml");
        FolderManifest::create(&path).unwrap();

        let manifest = FolderManifest::load(&path).unwrap();
        assert!(manifest.headers.is_empty());
        assert!(manifest.auth.is_none());
    }

    #[test]
    fn create_refuses_to_overwrite_an_existing_manifest() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("folder.toml");
        FolderManifest::create(&path).unwrap();

        let err = FolderManifest::create(&path).unwrap_err();
        assert!(matches!(err, FormatError::AlreadyExists { .. }));
    }

    #[test]
    fn parses_headers_and_auth() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("folder.toml");
        std::fs::write(
            &path,
            r#"
            [[headers]]
            name = "X-Api-Version"
            value = "1"

            [auth]
            type = "bearer"
            token = "{{token}}"
            "#,
        )
        .unwrap();

        let manifest = FolderManifest::load(&path).unwrap();
        assert_eq!(manifest.headers[0].name, "X-Api-Version");
        assert!(matches!(manifest.auth, Some(AuthSpec::Bearer { .. })));
    }

    #[test]
    fn load_folder_chain_finds_only_the_root_manifest_when_request_is_at_the_root() {
        let dir = tempdir().unwrap();
        FolderManifest::create(&dir.path().join("folder.toml")).unwrap();

        let chain = load_folder_chain(dir.path(), dir.path()).unwrap();
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn load_folder_chain_walks_nested_directories_root_to_leaf() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("folder.toml"),
            "[[headers]]\nname = \"A\"\nvalue = \"root\"\n",
        )
        .unwrap();
        let nested = dir.path().join("auth").join("v2");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            dir.path().join("auth").join("folder.toml"),
            "[[headers]]\nname = \"A\"\nvalue = \"auth\"\n",
        )
        .unwrap();
        std::fs::write(
            nested.join("folder.toml"),
            "[[headers]]\nname = \"A\"\nvalue = \"v2\"\n",
        )
        .unwrap();

        let chain = load_folder_chain(dir.path(), &nested).unwrap();
        let values: Vec<_> = chain.iter().map(|f| f.headers[0].value.clone()).collect();
        assert_eq!(values, vec!["root", "auth", "v2"]);
    }

    #[test]
    fn load_folder_chain_skips_directories_without_a_manifest() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("users");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            nested.join("folder.toml"),
            "[[headers]]\nname = \"A\"\nvalue = \"users\"\n",
        )
        .unwrap();

        let chain = load_folder_chain(dir.path(), &nested).unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].headers[0].value, "users");
    }

    #[test]
    fn load_folder_chain_is_empty_when_no_folder_toml_exists_anywhere() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("users");
        std::fs::create_dir_all(&nested).unwrap();

        let chain = load_folder_chain(dir.path(), &nested).unwrap();
        assert!(chain.is_empty());
    }
}
