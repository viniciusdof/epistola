//! Tracks recently opened collections across launches, for a client's
//! "recent collections" list. Internal cache, not user-hand-edited, so it's
//! a JSON blob (like `history.rs`) rather than TOML.

use std::path::{Path, PathBuf};

use epistola_format::global_config_dir;

use crate::error::EngineError;

const MAX_ENTRIES: usize = 10;
const FILE_NAME: &str = "recent-collections.json";

fn io_err(path: PathBuf) -> impl FnOnce(std::io::Error) -> EngineError {
    move |source| EngineError::Io { path, source }
}

/// Most-recently-opened first. Entries whose directory no longer exists are
/// silently dropped.
pub fn list() -> Result<Vec<PathBuf>, EngineError> {
    list_in(&global_config_dir()?)
}

/// Moves `path` to the front of the list (inserting it if new), dropping
/// anything past `MAX_ENTRIES`.
pub fn record(path: &Path) -> Result<(), EngineError> {
    record_in(&global_config_dir()?, path)
}

fn list_in(config_dir: &Path) -> Result<Vec<PathBuf>, EngineError> {
    let path = config_dir.join(FILE_NAME);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path).map_err(io_err(path))?;
    let paths: Vec<PathBuf> = serde_json::from_str(&content).unwrap_or_default();
    Ok(paths.into_iter().filter(|p| p.is_dir()).collect())
}

fn record_in(config_dir: &Path, path: &Path) -> Result<(), EngineError> {
    let mut paths = list_in(config_dir).unwrap_or_default();
    paths.retain(|p| p != path);
    paths.insert(0, path.to_path_buf());
    paths.truncate(MAX_ENTRIES);

    std::fs::create_dir_all(config_dir).map_err(io_err(config_dir.to_path_buf()))?;
    let file_path = config_dir.join(FILE_NAME);
    let json = serde_json::to_string_pretty(&paths)?;
    std::fs::write(&file_path, json).map_err(io_err(file_path))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn list_is_empty_when_no_file_exists_yet() {
        let config_dir = tempdir().unwrap();
        assert!(list_in(config_dir.path()).unwrap().is_empty());
    }

    #[test]
    fn record_then_list_round_trips_and_puts_newest_first() {
        let config_dir = tempdir().unwrap();
        let collection_a = tempdir().unwrap();
        let collection_b = tempdir().unwrap();

        record_in(config_dir.path(), collection_a.path()).unwrap();
        record_in(config_dir.path(), collection_b.path()).unwrap();

        let paths = list_in(config_dir.path()).unwrap();
        assert_eq!(
            paths,
            vec![
                collection_b.path().to_path_buf(),
                collection_a.path().to_path_buf()
            ]
        );
    }

    #[test]
    fn re_recording_an_existing_entry_moves_it_to_the_front_without_duplicating() {
        let config_dir = tempdir().unwrap();
        let collection_a = tempdir().unwrap();
        let collection_b = tempdir().unwrap();

        record_in(config_dir.path(), collection_a.path()).unwrap();
        record_in(config_dir.path(), collection_b.path()).unwrap();
        record_in(config_dir.path(), collection_a.path()).unwrap();

        let paths = list_in(config_dir.path()).unwrap();
        assert_eq!(
            paths,
            vec![
                collection_a.path().to_path_buf(),
                collection_b.path().to_path_buf()
            ]
        );
    }

    #[test]
    fn entries_whose_directory_no_longer_exists_are_dropped() {
        let config_dir = tempdir().unwrap();
        let vanished = tempdir().unwrap().path().to_path_buf();
        let still_here = tempdir().unwrap();

        record_in(config_dir.path(), &vanished).unwrap();
        record_in(config_dir.path(), still_here.path()).unwrap();

        let paths = list_in(config_dir.path()).unwrap();
        assert_eq!(paths, vec![still_here.path().to_path_buf()]);
    }

    #[test]
    fn list_caps_at_max_entries() {
        let config_dir = tempdir().unwrap();
        let dirs: Vec<_> = (0..MAX_ENTRIES + 3).map(|_| tempdir().unwrap()).collect();
        for dir in &dirs {
            record_in(config_dir.path(), dir.path()).unwrap();
        }
        assert_eq!(list_in(config_dir.path()).unwrap().len(), MAX_ENTRIES);
    }
}
