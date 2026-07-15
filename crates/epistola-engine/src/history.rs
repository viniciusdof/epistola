use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use epistola_core::{Header, Request, Response};

use crate::error::EngineError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggedPair {
    pub name: String,
    pub value: String,
}

impl From<&Header> for LoggedPair {
    fn from(header: &Header) -> Self {
        LoggedPair {
            name: header.name.clone(),
            value: header.value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggedRequest {
    pub method: String,
    pub url: String,
    pub query: Vec<LoggedPair>,
    pub headers: Vec<LoggedPair>,
    pub body: Option<String>,
}

impl From<&Request> for LoggedRequest {
    fn from(request: &Request) -> Self {
        LoggedRequest {
            method: request.method.as_str().to_string(),
            url: request.url.clone(),
            query: request
                .query
                .iter()
                .map(|(name, value)| LoggedPair {
                    name: name.clone(),
                    value: value.clone(),
                })
                .collect(),
            headers: request.headers.iter().map(LoggedPair::from).collect(),
            body: std::str::from_utf8(request.body.as_bytes())
                .ok()
                .map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggedResponse {
    pub status: u16,
    pub duration_ms: u128,
    pub headers: Vec<LoggedPair>,
    pub body: Option<String>,
}

impl From<&Response> for LoggedResponse {
    fn from(response: &Response) -> Self {
        LoggedResponse {
            status: response.status,
            duration_ms: response.duration.as_millis(),
            headers: response.headers.iter().map(LoggedPair::from).collect(),
            body: response.body_as_str().ok().map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: time::OffsetDateTime,
    pub request: LoggedRequest,
    pub response: LoggedResponse,
}

/// Path to a collection's history log, relative to its root. Created
/// lazily on the first [`append_entry`] call — `epistola init` only
/// gitignores this path, it doesn't create it.
pub fn log_path(collection_root: &Path) -> PathBuf {
    collection_root.join(".epistola").join("history.ndjson")
}

/// Appends one entry, creating `.epistola/` and the log file on first
/// write. Never truncates or rewrites existing lines.
pub fn append_entry(
    collection_root: &Path,
    request: &Request,
    response: &Response,
) -> Result<(), EngineError> {
    let path = log_path(collection_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| EngineError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let entry = HistoryEntry {
        timestamp: time::OffsetDateTime::now_utc(),
        request: LoggedRequest::from(request),
        response: LoggedResponse::from(response),
    };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|source| EngineError::Io {
            path: path.clone(),
            source,
        })?;
    writeln!(file, "{}", serde_json::to_string(&entry)?)
        .map_err(|source| EngineError::Io { path, source })
}

/// Reads back every logged entry, most-recent-first (`entries[0]` is the
/// last request run — the index `history show` calls "1"). An empty `Vec`
/// if the log doesn't exist yet.
pub fn read_entries(collection_root: &Path) -> Result<Vec<HistoryEntry>, EngineError> {
    let path = log_path(collection_root);
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path).map_err(|source| EngineError::Io {
        path: path.clone(),
        source,
    })?;
    let mut entries = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<Vec<HistoryEntry>, _>>()?;
    entries.reverse();
    Ok(entries)
}

/// Deletes the log file; a no-op if it doesn't exist.
pub fn clear(collection_root: &Path) -> Result<(), EngineError> {
    let path = log_path(collection_root);
    if path.is_file() {
        std::fs::remove_file(&path).map_err(|source| EngineError::Io { path, source })?;
    }
    Ok(())
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
        assert_eq!(entries[0].request.url, "https://x.test/second");
        assert_eq!(entries[1].request.url, "https://x.test/first");
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
        assert_eq!(entries[0].response.body.as_deref(), Some("ok"));
        assert_eq!(entries[0].response.status, 200);
    }

    #[test]
    fn append_entry_fails_when_the_dot_epistola_path_is_a_file() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".epistola"), "not a directory").unwrap();

        assert!(append_entry(dir.path(), &request(), &response()).is_err());
    }

    #[test]
    fn read_entries_parses_a_literal_ndjson_line_in_the_on_disk_shape() {
        let dir = tempdir().unwrap();
        let line = concat!(
            r#"{"timestamp":"2026-01-01T00:00:00Z","#,
            r#""request":{"method":"GET","url":"https://x.test","query":[],"headers":[{"name":"X-Test","value":"1"}],"body":null},"#,
            r#""response":{"status":200,"duration_ms":7,"headers":[],"body":"ok"}}"#,
        );
        std::fs::create_dir_all(dir.path().join(".epistola")).unwrap();
        std::fs::write(log_path(dir.path()), format!("{line}\n")).unwrap();

        let entries = read_entries(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].request.url, "https://x.test");
        assert_eq!(entries[0].response.status, 200);
        assert_eq!(entries[0].response.body.as_deref(), Some("ok"));
    }

    #[test]
    fn read_entries_fails_explicitly_on_a_corrupted_line_instead_of_defaulting() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".epistola")).unwrap();
        std::fs::write(log_path(dir.path()), "not json at all\n").unwrap();

        assert!(read_entries(dir.path()).is_err());
    }
}
