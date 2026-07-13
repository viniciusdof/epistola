use std::path::{Path, PathBuf};

use epistola_format::LoadedCollection;

use crate::error::EngineError;
use crate::resolve::resolve_with_base_resolver;

/// Turns an arbitrary request name into a filesystem-safe slug:
/// lowercased, non-alphanumeric runs collapsed to a single `-`, trimmed.
/// Falls back to `"request"` if nothing alphanumeric remains.
pub fn slugify(name: &str) -> String {
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

/// Recursively finds every `*.req.toml` file under `root`.
pub fn find_request_files(root: &Path) -> Result<Vec<PathBuf>, EngineError> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|source| EngineError::Io {
            path: dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| EngineError::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
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

pub struct LintIssue {
    pub path: PathBuf,
    pub message: String,
}

pub struct LintReport {
    pub checked: usize,
    pub issues: Vec<LintIssue>,
}

/// Resolves every request file in the collection, collecting an issue per
/// file that fails instead of stopping at the first.
pub fn lint_collection(
    collection: &LoadedCollection,
    environment: Option<&str>,
) -> Result<LintReport, EngineError> {
    let base_resolver = collection.resolver_for_environment(environment)?;

    let mut checked = 0usize;
    let mut issues = Vec::new();
    for path in find_request_files(&collection.root)? {
        checked += 1;
        let result = resolve_with_base_resolver(
            &path,
            collection,
            &base_resolver,
            std::collections::BTreeMap::new(),
        );
        if let Err(err) = result {
            issues.push(LintIssue {
                path,
                message: err.to_string(),
            });
        }
    }

    Ok(LintReport { checked, issues })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_format::{CollectionManifest, RequestFile};
    use tempfile::tempdir;

    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn slugify_lowercases_and_dashes_whitespace() {
        assert_eq!(slugify("List Users"), "list-users");
    }

    #[test]
    fn slugify_collapses_repeated_separators() {
        assert_eq!(slugify("List   Users!!"), "list-users");
    }

    #[test]
    fn slugify_falls_back_when_nothing_alphanumeric_remains() {
        assert_eq!(slugify("!!!"), "request");
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
    fn lint_reports_zero_issues_for_a_fully_resolvable_collection() {
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

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        let report = lint_collection(&collection, None).unwrap();
        assert_eq!(report.checked, 2);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn lint_collects_issues_from_every_bad_file_instead_of_stopping_at_the_first() {
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

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        let report = lint_collection(&collection, None).unwrap();
        assert_eq!(report.issues.len(), 2);
    }

    #[test]
    fn lint_fails_a_request_whose_inherited_folder_auth_token_is_unresolvable() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "auth/folder.toml",
            "[auth]\ntype = \"bearer\"\ntoken = \"{{missing_token}}\"\n",
        );
        write(
            dir.path(),
            "auth/login.req.toml",
            "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"https://x.test\"\n",
        );

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        let report = lint_collection(&collection, None).unwrap();
        assert_eq!(report.issues.len(), 1);
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

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        assert!(!lint_collection(&collection, None)
            .unwrap()
            .issues
            .is_empty());
        assert!(lint_collection(&collection, Some("dev"))
            .unwrap()
            .issues
            .is_empty());
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

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        assert!(lint_collection(&collection, Some("dev"))
            .unwrap()
            .issues
            .is_empty());
    }
}
