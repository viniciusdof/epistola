use std::path::{Path, PathBuf};

use crate::error::FormatError;

/// Walks up from `start` looking for `epistola.toml`, like `git`/`cargo`.
pub fn find_collection_root(start: &Path) -> Result<PathBuf, FormatError> {
    let mut dir = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };

    loop {
        if dir.join("epistola.toml").is_file() {
            return Ok(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => {
                return Err(FormatError::CollectionRootNotFound {
                    start: start.to_path_buf(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn finds_the_root_when_starting_at_the_root_itself() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("epistola.toml"), "name = \"n\"\n").unwrap();
        assert_eq!(find_collection_root(dir.path()).unwrap(), dir.path());
    }

    #[test]
    fn finds_the_root_from_a_nested_subdirectory() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("epistola.toml"), "name = \"n\"\n").unwrap();
        let nested = dir.path().join("users").join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(find_collection_root(&nested).unwrap(), dir.path());
    }

    #[test]
    fn accepts_a_file_path_as_the_starting_point() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("epistola.toml"), "name = \"n\"\n").unwrap();
        let file = dir.path().join("users").join("list.req.toml");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "").unwrap();
        assert_eq!(find_collection_root(&file).unwrap(), dir.path());
    }

    #[test]
    fn errors_when_no_epistola_toml_exists_up_to_the_filesystem_root() {
        let dir = tempdir().unwrap();
        let result = find_collection_root(dir.path());
        assert!(matches!(
            result,
            Err(FormatError::CollectionRootNotFound { .. })
        ));
    }
}
