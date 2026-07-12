use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use epistola_format::{LoadedCollection, RequestFile};

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

pub fn run(args: RequestArgs, cwd: &Path) -> Result<()> {
    match args.action {
        RequestAction::New(a) => new(a, cwd),
        RequestAction::List(a) => list(a, cwd),
        RequestAction::Show(a) => show(a),
        RequestAction::Validate(a) => validate(a),
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
    let request = unresolved.resolve(&resolver)?;

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
}
