use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use epistola_core::{Request, Response};

use crate::output::{request_to_json, response_to_json};

/// Path to a collection's history log, relative to its root. Created
/// lazily on the first [`append_entry`] call — `epistola init` only
/// gitignores this path, it doesn't create it.
pub fn log_path(collection_root: &Path) -> PathBuf {
    collection_root.join(".epistola").join("history.ndjson")
}

/// Appends one entry, creating `.epistola/` and the log file on first
/// write. Never truncates or rewrites existing lines.
pub fn append_entry(collection_root: &Path, request: &Request, response: &Response) -> Result<()> {
    let path = log_path(collection_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }

    let entry = serde_json::json!({
        "timestamp": now_rfc3339(),
        "request": request_to_json(request),
        "response": response_to_json(response),
    });

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open '{}'", path.display()))?;
    writeln!(file, "{}", serde_json::to_string(&entry)?)
        .with_context(|| format!("failed to write to '{}'", path.display()))
}

/// Reads back every logged entry, most-recent-first (`entries[0]` is the
/// last request run — the index `history show` calls "1"). An empty `Vec`
/// if the log doesn't exist yet.
pub fn read_entries(collection_root: &Path) -> Result<Vec<serde_json::Value>> {
    let path = log_path(collection_root);
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read '{}'", path.display()))?;
    let mut entries = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<Vec<serde_json::Value>, _>>()
        .with_context(|| format!("failed to parse '{}'", path.display()))?;
    entries.reverse();
    Ok(entries)
}

/// Deletes the log file; a no-op if it doesn't exist.
pub fn clear(collection_root: &Path) -> Result<()> {
    let path = log_path(collection_root);
    if path.is_file() {
        std::fs::remove_file(&path)
            .with_context(|| format!("failed to remove '{}'", path.display()))?;
    }
    Ok(())
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_core::Method;
    use tempfile::tempdir;

    use super::*;

    fn request() -> Request {
        Request::new(Method::Get, "https://x.test/users").header("X-Test", "1")
    }

    fn response() -> Response {
        Response {
            status: 200,
            headers: Vec::new(),
            body: b"ok".to_vec(),
            duration: std::time::Duration::from_millis(42),
        }
    }

    #[test]
    fn append_entry_creates_the_dot_epistola_dir_lazily() {
        let dir = tempdir().unwrap();
        assert!(!dir.path().join(".epistola").is_dir());

        append_entry(dir.path(), &request(), &response()).unwrap();

        assert!(log_path(dir.path()).is_file());
    }

    #[test]
    fn multiple_appends_accumulate_in_file_order() {
        let dir = tempdir().unwrap();
        append_entry(dir.path(), &request(), &response()).unwrap();
        append_entry(dir.path(), &request(), &response()).unwrap();
        append_entry(dir.path(), &request(), &response()).unwrap();

        let content = std::fs::read_to_string(log_path(dir.path())).unwrap();
        assert_eq!(content.lines().count(), 3);
    }

    #[test]
    fn read_entries_is_most_recent_first() {
        let dir = tempdir().unwrap();
        append_entry(
            dir.path(),
            &Request::new(Method::Get, "https://x.test/first"),
            &response(),
        )
        .unwrap();
        append_entry(
            dir.path(),
            &Request::new(Method::Get, "https://x.test/second"),
            &response(),
        )
        .unwrap();

        let entries = read_entries(dir.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["request"]["url"], "https://x.test/second");
        assert_eq!(entries[1]["request"]["url"], "https://x.test/first");
    }

    #[test]
    fn read_entries_on_a_missing_log_is_empty() {
        let dir = tempdir().unwrap();
        assert!(read_entries(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn clear_removes_the_log_file() {
        let dir = tempdir().unwrap();
        append_entry(dir.path(), &request(), &response()).unwrap();
        assert!(log_path(dir.path()).is_file());

        clear(dir.path()).unwrap();

        assert!(!log_path(dir.path()).is_file());
    }

    #[test]
    fn clear_is_a_noop_when_no_log_exists() {
        let dir = tempdir().unwrap();
        clear(dir.path()).unwrap();
        assert!(!log_path(dir.path()).is_dir());
    }

    #[test]
    fn append_entry_records_the_response_body_and_status() {
        let dir = tempdir().unwrap();
        append_entry(dir.path(), &request(), &response()).unwrap();

        let entries = read_entries(dir.path()).unwrap();
        assert_eq!(entries[0]["response"]["body"], "ok");
        assert_eq!(entries[0]["response"]["status"], 200);
    }
}
