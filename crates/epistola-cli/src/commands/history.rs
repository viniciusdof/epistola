use std::path::Path;

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use epistola_engine::discovery::discover_collection;
use epistola_engine::history::{self, HistoryEntry};

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

fn list(args: ListArgs, cwd: &Path) -> Result<()> {
    let collection = discover_collection(cwd)?;
    let mut entries = history::read_entries(&collection.root)?;
    if let Some(limit) = args.limit {
        entries.truncate(limit);
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
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
    let collection = discover_collection(cwd)?;
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
    let collection = discover_collection(cwd)?;
    history::clear(&collection.root)?;
    println!("Cleared history for this collection.");
    Ok(())
}

fn format_entry_summary(index: usize, entry: &HistoryEntry) -> String {
    format!(
        "{:>3}  {}  {} {}  {} ({}ms)",
        index,
        entry.timestamp,
        entry.request.method,
        entry.request.url,
        entry.response.status,
        entry.response.duration_ms
    )
}

fn format_entry_text(entry: &HistoryEntry) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}\n", entry.timestamp));
    out.push_str(&format!("{} {}\n", entry.request.method, entry.request.url));

    for q in &entry.request.query {
        out.push_str(&format!("  ?{}={}\n", q.name, q.value));
    }
    for h in &entry.request.headers {
        out.push_str(&format!("{}: {}\n", h.name, h.value));
    }
    out.push('\n');
    if let Some(body) = &entry.request.body {
        if !body.is_empty() {
            out.push_str(body);
            out.push('\n');
        }
    }
    out.push('\n');

    out.push_str(&format!(
        "HTTP {} ({}ms)\n",
        entry.response.status, entry.response.duration_ms
    ));
    for h in &entry.response.headers {
        out.push_str(&format!("{}: {}\n", h.name, h.value));
    }
    out.push('\n');
    if let Some(body) = &entry.response.body {
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
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        seed_entry(dir.path());
        let entry = &history::read_entries(dir.path()).unwrap()[0];

        let out = format_entry_summary(1, entry);
        assert!(out.contains("GET https://x.test/users"));
        assert!(out.contains("200"));
    }
}
