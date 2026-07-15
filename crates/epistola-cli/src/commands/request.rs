use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use epistola_engine::discovery::discover_collection;
use epistola_engine::history::LoggedRequest;
use epistola_engine::requests::{
    create_request, delete_request, duplicate_request, lint_collection, list_requests,
    rename_request,
};
use epistola_engine::resolve::load_and_resolve;
use epistola_engine::EngineError;
use epistola_format::RequestFile;

use crate::output;

#[derive(Args, Debug)]
pub struct RequestArgs {
    #[command(subcommand)]
    pub action: RequestAction,
}

#[derive(Subcommand, Debug)]
pub enum RequestAction {
    /// Create a new request in the current collection
    New(NewArgs),
    /// List every request in the current collection
    List(ListArgs),
    /// Print a request, raw or resolved
    Show(ShowArgs),
    /// Check that a request file parses
    Validate(ValidateArgs),
    /// Check that every request in the collection resolves cleanly (unknown
    /// variables, missing multipart files) — not just that it parses
    Lint(LintArgs),
    /// Delete a request file
    Delete(DeleteArgs),
    /// Rename a request, updating both its file name and its `name` field
    Rename(RenameArgs),
    /// Copy a request to a new file under a new name, in the same directory
    Duplicate(DuplicateArgs),
}

#[derive(Args, Debug)]
pub struct NewArgs {
    pub name: String,
    #[arg(long, default_value = "GET")]
    pub method: String,
    #[arg(long)]
    pub url: String,
    /// Folder (relative to the collection root) to create the request in
    #[arg(long, default_value = "")]
    pub dir: String,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    pub path: PathBuf,
    /// Interpolate variables and fold in auth, instead of printing the raw file
    #[arg(long)]
    pub resolved: bool,
    /// Environment to resolve against (only meaningful with --resolved)
    #[arg(long)]
    pub env: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ValidateArgs {
    pub path: PathBuf,
}

#[derive(Args, Debug)]
pub struct LintArgs {
    /// Environment to resolve each request's variables against
    #[arg(long)]
    pub env: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct DeleteArgs {
    pub path: PathBuf,
}

#[derive(Args, Debug)]
pub struct RenameArgs {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Args, Debug)]
pub struct DuplicateArgs {
    pub path: PathBuf,
    pub name: String,
}

pub fn run(args: RequestArgs, cwd: &Path) -> Result<()> {
    match args.action {
        RequestAction::New(a) => new(a, cwd),
        RequestAction::List(a) => list(a, cwd),
        RequestAction::Show(a) => show(a),
        RequestAction::Validate(a) => validate(a),
        RequestAction::Lint(a) => lint(a, cwd),
        RequestAction::Delete(a) => delete(a),
        RequestAction::Rename(a) => rename(a),
        RequestAction::Duplicate(a) => duplicate(a),
    }
}

fn new(args: NewArgs, cwd: &Path) -> Result<()> {
    let path = create_request(cwd, &args.dir, &args.name, &args.method, &args.url)?;
    println!("Created {}", path.display());
    Ok(())
}

fn list(args: ListArgs, cwd: &Path) -> Result<()> {
    let collection = discover_collection(cwd)?;
    let listing = list_requests(&collection)?;

    for invalid in &listing.invalid {
        eprintln!(
            "warning: failed to load '{}': {}",
            invalid.abs_path.display(),
            invalid.error
        );
    }

    if args.json {
        let json: Vec<_> = listing
            .requests
            .iter()
            .map(|r| {
                serde_json::json!({
                    "path": r.rel_path.display().to_string(),
                    "name": r.name,
                    "method": r.method.as_str(),
                    "url": r.url,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else if listing.requests.is_empty() {
        println!("No requests found in this collection.");
    } else {
        for r in &listing.requests {
            println!(
                "{:<8} {:<40} {}",
                r.method.as_str(),
                r.rel_path.display(),
                r.url
            );
        }
    }

    Ok(())
}

fn show(args: ShowArgs) -> Result<()> {
    if !args.resolved {
        return show_raw(&args);
    }

    let collection = discover_collection(&args.path)?;
    let resolved = load_and_resolve(
        &args.path,
        &collection,
        args.env.as_deref(),
        std::collections::BTreeMap::new(),
    )?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&LoggedRequest::from(&resolved.request))?
        );
    } else {
        print!("{}", output::format_request(&resolved.request));
    }

    Ok(())
}

fn show_raw(args: &ShowArgs) -> Result<()> {
    if args.json {
        let file = RequestFile::load(&args.path)
            .with_context(|| format!("failed to load '{}'", args.path.display()))?;
        println!("{}", serde_json::to_string_pretty(&file)?);
        return Ok(());
    }

    // Print verbatim, not round-tripped through the parsed struct.
    let content = std::fs::read_to_string(&args.path)
        .with_context(|| format!("failed to read '{}'", args.path.display()))?;
    print!("{content}");
    Ok(())
}

fn validate(args: ValidateArgs) -> Result<()> {
    RequestFile::load(&args.path)
        .with_context(|| format!("'{}' is not a valid request file", args.path.display()))?;
    println!("{} is valid", args.path.display());
    Ok(())
}

fn lint(args: LintArgs, cwd: &Path) -> Result<()> {
    let collection = discover_collection(cwd)?;
    let report = lint_collection(&collection, args.env.as_deref())?;

    if args.json {
        let json = serde_json::json!({
            "checked": report.checked,
            "errors": report.issues.iter().map(|issue| serde_json::json!({
                "path": issue.path.display().to_string(),
                "error": issue.message,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!(
            "Checked {} request file(s), {} error(s)",
            report.checked,
            report.issues.len()
        );
        for issue in &report.issues {
            println!("{}: {}", issue.path.display(), issue.message);
        }
    }

    if report.issues.is_empty() {
        Ok(())
    } else {
        Err(EngineError::LintFailed(report.issues.len()).into())
    }
}

fn delete(args: DeleteArgs) -> Result<()> {
    delete_request(&args.path)?;
    println!("Deleted {}", args.path.display());
    Ok(())
}

fn rename(args: RenameArgs) -> Result<()> {
    let new_path = rename_request(&args.path, &args.name)?;
    println!("Renamed {} to {}", args.path.display(), new_path.display());
    Ok(())
}

fn duplicate(args: DuplicateArgs) -> Result<()> {
    let new_path = duplicate_request(&args.path, &args.name)?;
    println!(
        "Duplicated {} to {}",
        args.path.display(),
        new_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_format::CollectionManifest;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn new_errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        let args = NewArgs {
            name: "List users".to_string(),
            method: "GET".to_string(),
            url: "https://x.test".to_string(),
            dir: String::new(),
        };
        assert!(new(args, dir.path()).is_err());
    }

    #[test]
    fn new_creates_a_slugified_request_file_at_the_collection_root() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        let args = NewArgs {
            name: "List Users".to_string(),
            method: "GET".to_string(),
            url: "https://x.test".to_string(),
            dir: String::new(),
        };
        new(args, dir.path()).unwrap();

        let path = dir.path().join("list-users.req.toml");
        let file = RequestFile::load(&path).unwrap();
        assert_eq!(file.request.name, "List Users");
        assert_eq!(file.request.url, "https://x.test");
    }

    #[test]
    fn new_creates_the_request_inside_the_given_subdirectory() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        let args = NewArgs {
            name: "Login".to_string(),
            method: "POST".to_string(),
            url: "https://x.test/login".to_string(),
            dir: "auth".to_string(),
        };
        new(args, dir.path()).unwrap();

        assert!(dir.path().join("auth/login.req.toml").is_file());
    }

    #[test]
    fn list_errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        assert!(list(ListArgs { json: false }, dir.path()).is_err());
    }

    #[test]
    fn list_finds_every_request_in_the_collection() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        RequestFile::create(&dir.path().join("a.req.toml"), "A", "GET", "https://x.test").unwrap();
        RequestFile::create(
            &dir.path().join("b.req.toml"),
            "B",
            "POST",
            "https://y.test",
        )
        .unwrap();

        // Exercises discovery + walk + parse; list() only prints, no return value to assert on.
        list(ListArgs { json: true }, dir.path()).unwrap();
    }

    #[test]
    fn show_raw_prints_the_file_verbatim() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        RequestFile::create(&path, "A", "GET", "https://x.test").unwrap();

        show(ShowArgs {
            path,
            resolved: false,
            env: None,
            json: false,
        })
        .unwrap();
    }

    #[test]
    fn show_resolved_interpolates_against_the_environment() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        std::fs::create_dir_all(dir.path().join("environments")).unwrap();
        std::fs::write(
            dir.path().join("environments/dev.toml"),
            "[variables]\nhost = \"x.test\"\n",
        )
        .unwrap();

        let path = dir.path().join("a.req.toml");
        RequestFile::create(&path, "A", "GET", "https://{{host}}").unwrap();

        show(ShowArgs {
            path,
            resolved: true,
            env: Some("dev".to_string()),
            json: true,
        })
        .unwrap();
    }

    #[test]
    fn show_resolved_propagates_an_unknown_variable_error() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        let path = dir.path().join("a.req.toml");
        RequestFile::create(&path, "A", "GET", "https://{{missing}}").unwrap();

        assert!(show(ShowArgs {
            path,
            resolved: true,
            env: None,
            json: false
        })
        .is_err());
    }

    #[test]
    fn validate_accepts_a_well_formed_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        RequestFile::create(&path, "A", "GET", "https://x.test").unwrap();
        assert!(validate(ValidateArgs { path }).is_ok());
    }

    #[test]
    fn validate_rejects_a_missing_file() {
        let dir = tempdir().unwrap();
        assert!(validate(ValidateArgs {
            path: dir.path().join("nope.req.toml")
        })
        .is_err());
    }

    #[test]
    fn lint_reports_zero_errors_for_a_fully_resolvable_collection() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        RequestFile::create(&dir.path().join("a.req.toml"), "A", "GET", "https://x.test").unwrap();

        assert!(lint(
            LintArgs {
                env: None,
                json: false
            },
            dir.path()
        )
        .is_ok());
    }

    #[test]
    fn lint_errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        assert!(lint(
            LintArgs {
                env: None,
                json: false
            },
            dir.path()
        )
        .is_err());
    }

    #[test]
    fn lint_returns_engine_error_lint_failed_with_the_issue_count() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        std::fs::write(
            dir.path().join("bad.req.toml"),
            "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"https://{{missing}}\"\n",
        )
        .unwrap();

        let err = lint(
            LintArgs {
                env: None,
                json: false,
            },
            dir.path(),
        )
        .unwrap_err();

        let engine_err = err.downcast_ref::<EngineError>().unwrap();
        assert!(matches!(engine_err, EngineError::LintFailed(1)));
    }

    #[test]
    fn delete_removes_the_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        RequestFile::create(&path, "A", "GET", "https://x.test").unwrap();

        delete(DeleteArgs { path: path.clone() }).unwrap();

        assert!(!path.is_file());
    }

    #[test]
    fn delete_errors_when_the_file_is_missing() {
        let dir = tempdir().unwrap();
        assert!(delete(DeleteArgs {
            path: dir.path().join("nope.req.toml")
        })
        .is_err());
    }

    #[test]
    fn rename_moves_the_file_and_updates_the_name_field() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        RequestFile::create(&path, "A", "GET", "https://x.test").unwrap();

        rename(RenameArgs {
            path: path.clone(),
            name: "List Users".to_string(),
        })
        .unwrap();

        assert!(!path.is_file());
        let new_path = dir.path().join("list-users.req.toml");
        let file = RequestFile::load(&new_path).unwrap();
        assert_eq!(file.request.name, "List Users");
    }

    #[test]
    fn rename_to_the_same_slug_overwrites_in_place() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        RequestFile::create(&path, "A", "GET", "https://x.test").unwrap();

        rename(RenameArgs {
            path: path.clone(),
            name: "A".to_string(),
        })
        .unwrap();

        let file = RequestFile::load(&path).unwrap();
        assert_eq!(file.request.name, "A");
    }

    #[test]
    fn duplicate_creates_a_new_file_and_preserves_the_original() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        RequestFile::create(&path, "A", "GET", "https://x.test").unwrap();

        duplicate(DuplicateArgs {
            path: path.clone(),
            name: "A Copy".to_string(),
        })
        .unwrap();

        assert!(path.is_file());
        let original = RequestFile::load(&path).unwrap();
        assert_eq!(original.request.name, "A");

        let new_path = dir.path().join("a-copy.req.toml");
        let copy = RequestFile::load(&new_path).unwrap();
        assert_eq!(copy.request.name, "A Copy");
        assert_eq!(copy.request.url, "https://x.test");
    }

    #[test]
    fn duplicate_refuses_to_overwrite_an_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        RequestFile::create(&path, "A", "GET", "https://x.test").unwrap();
        RequestFile::create(&dir.path().join("b.req.toml"), "B", "GET", "https://y.test").unwrap();

        assert!(duplicate(DuplicateArgs {
            path,
            name: "B".to_string(),
        })
        .is_err());
    }

    #[test]
    fn duplicate_onto_the_same_name_errors_instead_of_overwriting_the_source() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        RequestFile::create(&path, "A", "GET", "https://x.test").unwrap();

        assert!(duplicate(DuplicateArgs {
            path,
            name: "A".to_string(),
        })
        .is_err());
    }
}
