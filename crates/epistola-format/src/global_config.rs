use std::collections::BTreeMap;
use std::path::Path;

use directories::ProjectDirs;

use crate::error::FormatError;
use crate::variables_file::load_with_secrets_sidecar;

/// Loads `<config_dir>/config.toml` + secrets sidecar. Missing is not an
/// error — global config is optional.
pub(crate) fn load_from_dir(config_dir: &Path) -> Result<BTreeMap<String, String>, FormatError> {
    let public_path = config_dir.join("config.toml");
    if !public_path.is_file() {
        return Ok(BTreeMap::new());
    }
    let secrets_path = config_dir.join("config.secrets.toml");
    load_with_secrets_sidecar(&public_path, &secrets_path)
}

/// Resolves the OS-idiomatic config directory and loads the global config.
pub fn load_global_config() -> Result<BTreeMap<String, String>, FormatError> {
    let dirs = ProjectDirs::from("", "", "epistola").ok_or(FormatError::NoHomeDirectory)?;
    load_from_dir(dirs.config_dir())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn returns_an_empty_map_when_no_config_file_exists() {
        let dir = tempdir().unwrap();
        let vars = load_from_dir(dir.path()).unwrap();
        assert!(vars.is_empty());
    }

    #[test]
    fn reads_the_public_file() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            "[variables]\nuser_agent = \"epistola\"\n",
        )
        .unwrap();
        let vars = load_from_dir(dir.path()).unwrap();
        assert_eq!(vars.get("user_agent").map(String::as_str), Some("epistola"));
    }

    #[test]
    fn merges_the_secrets_sidecar() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            "[variables]\nk = \"public\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("config.secrets.toml"),
            "[variables]\nk = \"secret\"\n",
        )
        .unwrap();
        let vars = load_from_dir(dir.path()).unwrap();
        assert_eq!(vars.get("k").map(String::as_str), Some("secret"));
    }

    #[test]
    fn load_global_config_does_not_error_on_a_real_machine() {
        // Contents vary by machine; just check it resolves without erroring.
        assert!(load_global_config().is_ok());
    }
}
