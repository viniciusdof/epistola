use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::FormatError;
use crate::toml_file::write_toml_file;
use crate::variables_file::{load_with_secrets_sidecar, upsert_variable, VariablesFile};

fn public_path(collection_root: &Path, name: &str) -> PathBuf {
    collection_root
        .join("environments")
        .join(format!("{name}.toml"))
}

fn secrets_path(collection_root: &Path, name: &str) -> PathBuf {
    collection_root
        .join("environments")
        .join(format!("{name}.secrets.toml"))
}

/// Loads `environments/<name>.toml` merged with its `.secrets.toml`
/// sidecar. Errors with `EnvironmentNotFound` only if neither file exists.
pub fn load_environment(
    collection_root: &Path,
    name: &str,
) -> Result<BTreeMap<String, String>, FormatError> {
    let public = public_path(collection_root, name);
    let secrets = secrets_path(collection_root, name);
    if !public.is_file() && !secrets.is_file() {
        return Err(FormatError::EnvironmentNotFound {
            name: name.to_string(),
            path: public,
        });
    }
    load_with_secrets_sidecar(&public, &secrets)
}

/// Creates an empty `environments/<name>.toml`.
pub fn create_environment(collection_root: &Path, name: &str) -> Result<PathBuf, FormatError> {
    let public = public_path(collection_root, name);
    if public.is_file() {
        return Err(FormatError::AlreadyExists { path: public });
    }
    write_toml_file(&public, &VariablesFile::default())?;
    Ok(public)
}

/// Sets `key = value` in the public file, or its `.secrets.toml` sidecar
/// if `secret`. Creates the file if missing; idempotent.
pub fn set_environment_variable(
    collection_root: &Path,
    name: &str,
    key: &str,
    value: &str,
    secret: bool,
) -> Result<PathBuf, FormatError> {
    let path = if secret {
        secrets_path(collection_root, name)
    } else {
        public_path(collection_root, name)
    };
    upsert_variable(&path, key, value)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    fn write(dir: &std::path::Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn loads_the_public_file() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "environments/dev.toml",
            "[variables]\nbase_url = \"https://dev.test\"\n",
        );
        let vars = load_environment(dir.path(), "dev").unwrap();
        assert_eq!(
            vars.get("base_url").map(String::as_str),
            Some("https://dev.test")
        );
    }

    #[test]
    fn merges_the_secrets_sidecar_and_secrets_win_on_conflict() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "environments/dev.toml",
            "[variables]\ntoken = \"public\"\nshared = \"a\"\n",
        );
        write(
            dir.path(),
            "environments/dev.secrets.toml",
            "[variables]\ntoken = \"secret\"\n",
        );
        let vars = load_environment(dir.path(), "dev").unwrap();
        assert_eq!(vars.get("token").map(String::as_str), Some("secret"));
        assert_eq!(vars.get("shared").map(String::as_str), Some("a"));
    }

    #[test]
    fn works_when_no_secrets_sidecar_exists() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "environments/dev.toml",
            "[variables]\nx = \"1\"\n",
        );
        let vars = load_environment(dir.path(), "dev").unwrap();
        assert_eq!(vars.len(), 1);
    }

    #[test]
    fn errors_with_environment_not_found_when_the_named_file_is_missing() {
        let dir = tempdir().unwrap();
        let err = load_environment(dir.path(), "prod").unwrap_err();
        assert!(matches!(err, FormatError::EnvironmentNotFound { name, .. } if name == "prod"));
    }

    #[test]
    fn create_environment_writes_an_empty_variables_table() {
        let dir = tempdir().unwrap();
        create_environment(dir.path(), "dev").unwrap();
        let vars = load_environment(dir.path(), "dev").unwrap();
        assert!(vars.is_empty());
    }

    #[test]
    fn create_environment_refuses_to_overwrite_an_existing_one() {
        let dir = tempdir().unwrap();
        create_environment(dir.path(), "dev").unwrap();
        let err = create_environment(dir.path(), "dev").unwrap_err();
        assert!(matches!(err, FormatError::AlreadyExists { .. }));
    }

    #[test]
    fn set_environment_variable_creates_the_file_if_missing() {
        let dir = tempdir().unwrap();
        set_environment_variable(dir.path(), "dev", "base_url", "https://dev.test", false).unwrap();
        let vars = load_environment(dir.path(), "dev").unwrap();
        assert_eq!(
            vars.get("base_url").map(String::as_str),
            Some("https://dev.test")
        );
    }

    #[test]
    fn set_environment_variable_with_secret_writes_to_the_sidecar_not_the_public_file() {
        let dir = tempdir().unwrap();
        set_environment_variable(dir.path(), "dev", "token", "s3cr3t", true).unwrap();

        let public = public_path(dir.path(), "dev");
        assert!(
            !public.is_file(),
            "secret write should not touch the public file"
        );

        let vars = load_environment(dir.path(), "dev").unwrap();
        assert_eq!(vars.get("token").map(String::as_str), Some("s3cr3t"));
    }

    #[test]
    fn set_environment_variable_is_idempotent_and_preserves_other_keys() {
        let dir = tempdir().unwrap();
        set_environment_variable(dir.path(), "dev", "a", "1", false).unwrap();
        set_environment_variable(dir.path(), "dev", "b", "2", false).unwrap();
        set_environment_variable(dir.path(), "dev", "a", "updated", false).unwrap();

        let vars = load_environment(dir.path(), "dev").unwrap();
        assert_eq!(vars.get("a").map(String::as_str), Some("updated"));
        assert_eq!(vars.get("b").map(String::as_str), Some("2"));
    }
}
