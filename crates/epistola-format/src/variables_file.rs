use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::FormatError;
use crate::toml_file::{read_toml_file, write_toml_file};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub(crate) struct VariablesFile {
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

/// Merges `public_path`'s `[variables]` with `secrets_path`'s (secrets
/// win). Neither file is required to exist.
pub(crate) fn load_with_secrets_sidecar(
    public_path: &Path,
    secrets_path: &Path,
) -> Result<BTreeMap<String, String>, FormatError> {
    let mut variables = if public_path.is_file() {
        read_toml_file::<VariablesFile>(public_path)?.variables
    } else {
        BTreeMap::new()
    };
    if secrets_path.is_file() {
        variables.extend(read_toml_file::<VariablesFile>(secrets_path)?.variables);
    }
    Ok(variables)
}

/// Confirms `input` parses as a `{ variables = { ... } }`-shaped TOML table
/// without persisting anything.
pub fn validate_variables_toml(input: &str) -> Result<(), FormatError> {
    toml::from_str::<VariablesFile>(input)?;
    Ok(())
}

/// Sets `key` in `path`'s `[variables]` table and writes it back. Loses
/// hand-written comments on round-trip (known limitation); `BTreeMap` keeps
/// key order stable so unrelated keys don't shuffle in the diff.
pub(crate) fn upsert_variable(path: &Path, key: &str, value: &str) -> Result<(), FormatError> {
    let mut file = if path.is_file() {
        read_toml_file::<VariablesFile>(path)?
    } else {
        VariablesFile::default()
    };
    file.variables.insert(key.to_string(), value.to_string());
    write_toml_file(path, &file)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn validate_variables_toml_accepts_a_variables_table() {
        assert!(validate_variables_toml("[variables]\nbase_url = \"https://dev.test\"\n").is_ok());
    }

    #[test]
    fn validate_variables_toml_rejects_invalid_toml() {
        assert!(validate_variables_toml("not valid [ toml").is_err());
    }

    #[test]
    fn upsert_creates_the_file_when_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dev.toml");
        upsert_variable(&path, "base_url", "https://dev.test").unwrap();
        let file: VariablesFile = read_toml_file(&path).unwrap();
        assert_eq!(
            file.variables.get("base_url").map(String::as_str),
            Some("https://dev.test")
        );
    }

    #[test]
    fn upsert_preserves_existing_keys_and_overwrites_the_target_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dev.toml");
        std::fs::write(&path, "[variables]\nkeep = \"a\"\nchange = \"old\"\n").unwrap();

        upsert_variable(&path, "change", "new").unwrap();

        let file: VariablesFile = read_toml_file(&path).unwrap();
        assert_eq!(file.variables.get("keep").map(String::as_str), Some("a"));
        assert_eq!(
            file.variables.get("change").map(String::as_str),
            Some("new")
        );
    }
}
