use std::path::Path;

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use epistola_format::LoadedCollection;
use serde_json::Value;

use crate::history;

#[derive(Args, Debug)]
pub struct HistoryArgs {
    #[command(subcommand)]
    pub action: HistoryAction,
}

#[derive(Subcommand, Debug)]
pub enum HistoryAction {
    /// List logged requests, most recent first
    List(ListArgs),
    /// Show the full stored request/response for entry N (1 = most recent)
    Show(ShowArgs),
    /// Delete the collection's history log
    Clear(ClearArgs),
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only show the N most recent entries
    #[arg(long)]
    pub limit: Option<usize>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// 1-based index into the history log, most recent first (see `history list`)
    pub index: usize,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ClearArgs {}

pub fn run(args: HistoryArgs, cwd: &Path) -> Result<()> {
    match args.action {
        HistoryAction::List(a) => list(a, cwd),
        HistoryAction::Show(a) => show(a, cwd),
        HistoryAction::Clear(a) => clear(a, cwd),
    }
}

fn current_collection(cwd: &Path) -> Result<LoadedCollection> {
    LoadedCollection::discover_from(cwd)
        .context("not inside a collection (no epistola.toml found in this or any parent directory)")
}

fn list(args: ListArgs, cwd: &Path) -> Result<()> {
    let collection = current_collection(cwd)?;
    let mut entries = history::read_entries(&collection.root)?;
    if let Some(limit) = args.limit {
        entries.truncate(limit);
    }

    if args.json {
        let json: Vec<Value> = entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let mut entry = entry.clone();
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert("index".to_string(), serde_json::json!(i + 1));
                }
                entry
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else if entries.is_empty() {
        println!("No history recorded in this collection.");
    } else {
        for (i, entry) in entries.iter().enumerate() {
            println!("{}", format_entry_summary(i + 1, entry));
        }
    }

    Ok(())
}

fn show(args: ShowArgs, cwd: &Path) -> Result<()> {
    let collection = current_collection(cwd)?;
    let entries = history::read_entries(&collection.root)?;

    if args.index == 0 {
        bail!("entry index must be 1 or greater");
    }
    let entry = entries.get(args.index - 1).with_context(|| {
        format!(
            "entry {} not found (log has {} entries)",
            args.index,
            entries.len()
        )
    })?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(entry)?);
    } else {
        print!("{}", format_entry_text(entry));
    }
    Ok(())
}

fn clear(_args: ClearArgs, cwd: &Path) -> Result<()> {
    let collection = current_collection(cwd)?;
    history::clear(&collection.root)?;
    println!("Cleared history for this collection.");
    Ok(())
}

fn format_entry_summary(index: usize, entry: &Value) -> String {
    let timestamp = entry.get("timestamp").and_then(Value::as_str).unwrap_or("");
    let method = entry["request"]
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("");
    let url = entry["request"]
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("");
    let status = entry["response"]
        .get("status")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let duration_ms = entry["response"]
        .get("duration_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    format!("{index:>3}  {timestamp}  {method} {url}  {status} ({duration_ms}ms)")
}

/// Walks the entry's known JSON shape directly rather than reconstructing
/// typed `epistola_core::Request`/`Response` values to reuse `output.rs`'s
/// formatters — round-tripping JSON back into those types (re-parsing
/// `Method`, rebuilding `Header` vecs, `duration_ms` back into a
/// `Duration`) would be more code and more failure modes than this.
fn format_entry_text(entry: &Value) -> String {
    let request = &entry["request"];
    let response = &entry["response"];
    let mut out = String::new();

    let timestamp = entry.get("timestamp").and_then(Value::as_str).unwrap_or("");
    out.push_str(&format!("{timestamp}\n"));

    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let url = request.get("url").and_then(Value::as_str).unwrap_or("");
    out.push_str(&format!("{method} {url}\n"));

    for q in request
        .get("query")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = q.get("name").and_then(Value::as_str).unwrap_or("");
        let value = q.get("value").and_then(Value::as_str).unwrap_or("");
        out.push_str(&format!("  ?{name}={value}\n"));
    }
    for h in request
        .get("headers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = h.get("name").and_then(Value::as_str).unwrap_or("");
        let value = h.get("value").and_then(Value::as_str).unwrap_or("");
        out.push_str(&format!("{name}: {value}\n"));
    }
    out.push('\n');
    if let Some(body) = request.get("body").and_then(Value::as_str) {
        if !body.is_empty() {
            out.push_str(body);
            out.push('\n');
        }
    }
    out.push('\n');

    let status = response.get("status").and_then(Value::as_u64).unwrap_or(0);
    let duration_ms = response
        .get("duration_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    out.push_str(&format!("HTTP {status} ({duration_ms}ms)\n"));
    for h in response
        .get("headers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = h.get("name").and_then(Value::as_str).unwrap_or("");
        let value = h.get("value").and_then(Value::as_str).unwrap_or("");
        out.push_str(&format!("{name}: {value}\n"));
    }
    out.push('\n');
    if let Some(body) = response.get("body").and_then(Value::as_str) {
        out.push_str(body);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_core::{Method, Request, Response};
    use tempfile::tempdir;

    use super::*;
    use epistola_format::CollectionManifest;

    fn seed_entry(root: &Path) {
        let request = Request::new(Method::Get, "https://x.test/users").header("X-Test", "1");
        let response = Response {
            status: 200,
            headers: Vec::new(),
            body: b"ok".to_vec(),
            duration: std::time::Duration::from_millis(7),
        };
        history::append_entry(root, &request, &response).unwrap();
    }

    #[test]
    fn list_errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        assert!(list(
            ListArgs {
                limit: None,
                json: false
            },
            dir.path()
        )
        .is_err());
    }

    #[test]
    fn list_works_when_the_log_is_empty() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        list(
            ListArgs {
                limit: None,
                json: false,
            },
            dir.path(),
        )
        .unwrap();
    }

    #[test]
    fn list_respects_limit() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        seed_entry(dir.path());
        seed_entry(dir.path());
        seed_entry(dir.path());

        let entries = history::read_entries(dir.path()).unwrap();
        assert_eq!(entries.len(), 3);

        list(
            ListArgs {
                limit: Some(1),
                json: true,
            },
            dir.path(),
        )
        .unwrap();
    }

    #[test]
    fn show_errors_when_the_index_is_out_of_range() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        seed_entry(dir.path());

        assert!(show(
            ShowArgs {
                index: 2,
                json: false
            },
            dir.path()
        )
        .is_err());
    }

    #[test]
    fn show_errors_when_the_index_is_zero() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        seed_entry(dir.path());

        assert!(show(
            ShowArgs {
                index: 0,
                json: false
            },
            dir.path()
        )
        .is_err());
    }

    #[test]
    fn show_prints_the_stored_request_and_response() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        seed_entry(dir.path());

        show(
            ShowArgs {
                index: 1,
                json: false,
            },
            dir.path(),
        )
        .unwrap();
    }

    #[test]
    fn clear_removes_the_log_file() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        seed_entry(dir.path());
        assert!(history::log_path(dir.path()).is_file());

        clear(ClearArgs {}, dir.path()).unwrap();

        assert!(!history::log_path(dir.path()).is_file());
    }

    #[test]
    fn clear_is_a_noop_when_no_log_exists() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        clear(ClearArgs {}, dir.path()).unwrap();
    }

    #[test]
    fn format_entry_summary_includes_method_url_and_status() {
        let entry = serde_json::json!({
            "timestamp": "2026-01-01T00:00:00Z",
            "request": {"method": "GET", "url": "https://x.test"},
            "response": {"status": 200, "duration_ms": 5},
        });
        let out = format_entry_summary(1, &entry);
        assert!(out.contains("GET https://x.test"));
        assert!(out.contains("200"));
    }
}
