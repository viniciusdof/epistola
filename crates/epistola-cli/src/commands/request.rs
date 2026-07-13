use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use epistola_core::LayeredVariableResolver;
use epistola_format::{LoadedCollection, RequestFile};

use crate::errors::CliError;
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

pub fn run(args: RequestArgs, cwd: &Path) -> Result<()> {
    match args.action {
        RequestAction::New(a) => new(a, cwd),
        RequestAction::List(a) => list(a, cwd),
        RequestAction::Show(a) => show(a),
        RequestAction::Validate(a) => validate(a),
        RequestAction::Lint(a) => lint(a, cwd),
        RequestAction::Delete(a) => delete(a),
        RequestAction::Rename(a) => rename(a),
    }
}

pub(crate) fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for c in name.to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "request".to_string()
    } else {
        slug
    }
}

fn current_collection(cwd: &Path) -> Result<LoadedCollection> {
    LoadedCollection::discover_from(cwd)
        .context("not inside a collection (no epistola.toml found in this or any parent directory)")
}

fn new(args: NewArgs, cwd: &Path) -> Result<()> {
    let collection = current_collection(cwd)?;

    let dir = if args.dir.is_empty() {
        collection.root.clone()
    } else {
        collection.root.join(&args.dir)
    };
    let path = dir.join(format!("{}.req.toml", slugify(&args.name)));

    RequestFile::create(&path, &args.name, &args.method, &args.url)
        .with_context(|| format!("failed to create '{}'", path.display()))?;

    println!("Created {}", path.display());
    Ok(())
}

fn find_request_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .with_context(|| format!("failed to read directory '{}'", dir.display()))?;
        for entry in entries {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".req.toml"))
            {
                found.push(path);
            }
        }
    }

    Ok(found)
}

fn list(args: ListArgs, cwd: &Path) -> Result<()> {
    let collection = current_collection(cwd)?;

    let mut entries = Vec::new();
    for path in find_request_files(&collection.root)? {
        let file = RequestFile::load(&path)
            .with_context(|| format!("failed to load '{}'", path.display()))?;
        let rel = path
            .strip_prefix(&collection.root)
            .unwrap_or(&path)
            .to_path_buf();
        entries.push((rel, file));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    if args.json {
        let json: Vec<_> = entries
            .iter()
            .map(|(rel, file)| {
                serde_json::json!({
                    "path": rel.display().to_string(),
                    "name": file.request.name,
                    "method": file.request.method,
                    "url": file.request.url,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else if entries.is_empty() {
        println!("No requests found in this collection.");
    } else {
        for (rel, file) in &entries {
            println!(
                "{:<8} {:<40} {}",
                file.request.method,
                rel.display(),
                file.request.url
            );
        }
    }

    Ok(())
}

fn show(args: ShowArgs) -> Result<()> {
    if !args.resolved {
        return show_raw(&args);
    }

    let file = RequestFile::load(&args.path)
        .with_context(|| format!("failed to load '{}'", args.path.display()))?;
    let collection = LoadedCollection::discover_from(&args.path).context(
        "not inside a collection (no epistola.toml found in this or any parent directory)",
    )?;

    let mut resolver = collection.resolver_for_environment(args.env.as_deref())?;
    let unresolved = file.to_unresolved();
    resolver = resolver.layer(unresolved.variables.clone());
    let base_dir = args
        .path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let request = unresolved.resolve(&resolver, base_dir)?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output::request_to_json(&request))?
        );
    } else {
        print!("{}", output::format_request(&request));
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

struct LintIssue {
    path: PathBuf,
    message: String,
}

fn lint(args: LintArgs, cwd: &Path) -> Result<()> {
    let collection = current_collection(cwd)?;
    let base_resolver = collection.resolver_for_environment(args.env.as_deref())?;

    let mut checked = 0usize;
    let mut issues = Vec::new();
    for path in find_request_files(&collection.root)? {
        checked += 1;
        if let Err(message) = lint_one(&path, &base_resolver) {
            issues.push(LintIssue { path, message });
        }
    }

    if args.json {
        let json = serde_json::json!({
            "checked": checked,
            "errors": issues.iter().map(|issue| serde_json::json!({
                "path": issue.path.display().to_string(),
                "error": issue.message,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!(
            "Checked {checked} request file(s), {} error(s)",
            issues.len()
        );
        for issue in &issues {
            println!("{}: {}", issue.path.display(), issue.message);
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(CliError::LintFailed(issues.len()).into())
    }
}

fn lint_one(path: &Path, base_resolver: &LayeredVariableResolver) -> Result<(), String> {
    let file = RequestFile::load(path).map_err(|err| err.to_string())?;
    let unresolved = file.to_unresolved();
    let resolver = base_resolver.clone().layer(unresolved.variables.clone());
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    unresolved
        .resolve(&resolver, base_dir)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

fn delete(args: DeleteArgs) -> Result<()> {
    if !args.path.is_file() {
        bail!("'{}' does not exist", args.path.display());
    }
    std::fs::remove_file(&args.path)
        .with_context(|| format!("failed to delete '{}'", args.path.display()))?;
    println!("Deleted {}", args.path.display());
    Ok(())
}

fn rename(args: RenameArgs) -> Result<()> {
    let mut file = RequestFile::load(&args.path)
        .with_context(|| format!("failed to load '{}'", args.path.display()))?;
    file.request.name = args.name.clone();

    let dir = args
        .path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let new_path = dir.join(format!("{}.req.toml", slugify(&args.name)));

    if new_path == args.path {
        file.save_at(&args.path)
            .with_context(|| format!("failed to save '{}'", args.path.display()))?;
    } else {
        file.create_at(&new_path)
            .with_context(|| format!("failed to create '{}'", new_path.display()))?;
        std::fs::remove_file(&args.path)
            .with_context(|| format!("failed to remove '{}'", args.path.display()))?;
    }

    println!("Renamed {} to {}", args.path.display(), new_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_format::CollectionManifest;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn slugify_lowercases_and_dashes_whitespace() {
        assert_eq!(super::slugify("List Users"), "list-users");
    }

    #[test]
    fn slugify_collapses_repeated_separators() {
        assert_eq!(super::slugify("List   Users!!"), "list-users");
    }

    #[test]
    fn slugify_falls_back_when_nothing_alphanumeric_remains() {
        assert_eq!(super::slugify("!!!"), "request");
    }

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
    fn find_request_files_finds_nested_req_toml_files_only() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("users")).unwrap();
        std::fs::write(dir.path().join("users/list.req.toml"), "").unwrap();
        std::fs::write(dir.path().join("environments.toml"), "").unwrap(); // not a request file

        let found = find_request_files(dir.path()).unwrap();
        assert_eq!(found, vec![dir.path().join("users/list.req.toml")]);
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

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn lint_reports_zero_errors_for_a_fully_resolvable_collection() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        RequestFile::create(&dir.path().join("a.req.toml"), "A", "GET", "https://x.test").unwrap();
        RequestFile::create(
            &dir.path().join("users/list.req.toml"),
            "B",
            "GET",
            "https://y.test",
        )
        .unwrap();

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
    fn lint_collects_errors_from_every_bad_file_instead_of_stopping_at_the_first() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        RequestFile::create(
            &dir.path().join("ok.req.toml"),
            "OK",
            "GET",
            "https://x.test",
        )
        .unwrap();
        write(
            dir.path(),
            "bad-var.req.toml",
            "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"https://{{missing}}\"\n",
        );
        write(
            dir.path(),
            "bad-multipart.req.toml",
            concat!(
                "[request]\n",
                "name = \"n\"\n",
                "method = \"POST\"\n",
                "url = \"https://x.test\"\n",
                "\n",
                "[request.body]\n",
                "type = \"multipart\"\n",
                "\n",
                "[[request.body.parts]]\n",
                "type = \"file\"\n",
                "name = \"avatar\"\n",
                "path = \"missing.png\"\n",
            ),
        );

        let err = lint(
            LintArgs {
                env: None,
                json: false,
            },
            dir.path(),
        )
        .unwrap_err();

        let cli_err = err.downcast_ref::<CliError>().unwrap();
        assert!(matches!(cli_err, CliError::LintFailed(2)));
    }

    #[test]
    fn lint_resolves_against_the_given_environment() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "environments/dev.toml",
            "[variables]\nhost = \"x.test\"\n",
        );
        RequestFile::create(
            &dir.path().join("a.req.toml"),
            "A",
            "GET",
            "https://{{host}}",
        )
        .unwrap();

        assert!(lint(
            LintArgs {
                env: None,
                json: false
            },
            dir.path()
        )
        .is_err());
        assert!(lint(
            LintArgs {
                env: Some("dev".to_string()),
                json: false
            },
            dir.path()
        )
        .is_ok());
    }

    #[test]
    fn lint_layers_request_level_variables_on_top_of_the_environment() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "environments/dev.toml",
            "[variables]\nhost = \"x.test\"\n",
        );
        write(
            dir.path(),
            "a.req.toml",
            concat!(
                "[request]\n",
                "name = \"n\"\n",
                "method = \"GET\"\n",
                "url = \"https://{{host}}/{{page}}\"\n",
                "\n",
                "[request.variables]\n",
                "page = \"1\"\n",
            ),
        );

        assert!(lint(
            LintArgs {
                env: Some("dev".to_string()),
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
}
