use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::FormatError;

pub(crate) fn read_toml_file<T: DeserializeOwned>(path: &Path) -> Result<T, FormatError> {
    let content = std::fs::read_to_string(path).map_err(|source| FormatError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&content).map_err(|source| FormatError::TomlAt {
        path: path.to_path_buf(),
        source,
    })
}

/// Serializes `value` to TOML and writes it to `path`, creating parent
/// directories as needed. Overwrites the file if it already exists.
pub(crate) fn write_toml_file<T: Serialize>(path: &Path, value: &T) -> Result<(), FormatError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| FormatError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let content = toml::to_string_pretty(value).map_err(|source| FormatError::TomlSerialize {
        path: path.to_path_buf(),
        source,
    })?;
    std::fs::write(path, content).map_err(|source| FormatError::Io {
        path: path.to_path_buf(),
        source,
    })
}
