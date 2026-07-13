use std::collections::BTreeMap;
use std::path::Path;

use epistola_core::{LayeredVariableResolver, Request};
use epistola_format::{load_folder_chain, LoadedCollection, RequestFile};

use crate::error::EngineError;

/// A saved request after interpolation, folder inheritance, and auth
/// encoding — ready to send.
pub struct ResolvedRequest {
    pub request: Request,
    pub history_enabled: bool,
}

/// Loads `path`, builds a fresh environment resolver, and resolves it.
/// For resolving many files against the same environment (e.g. linting a
/// whole collection), build the resolver once via
/// [`LoadedCollection::resolver_for_environment`] and call
/// [`resolve_with_base_resolver`] per file instead — re-deriving the
/// resolver (which re-reads global config and the environment file) per
/// file would be wasteful.
pub fn load_and_resolve(
    path: &Path,
    collection: &LoadedCollection,
    environment: Option<&str>,
    extra_vars: BTreeMap<String, String>,
) -> Result<ResolvedRequest, EngineError> {
    let base_resolver = collection.resolver_for_environment(environment)?;
    resolve_with_base_resolver(path, collection, &base_resolver, extra_vars)
}

pub fn resolve_with_base_resolver(
    path: &Path,
    collection: &LoadedCollection,
    base_resolver: &LayeredVariableResolver,
    extra_vars: BTreeMap<String, String>,
) -> Result<ResolvedRequest, EngineError> {
    let file = RequestFile::load(path)?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

    let chain = load_folder_chain(&collection.root, base_dir)?;
    let unresolved = file.to_unresolved().with_folder_inheritance(&chain);
    let mut resolver = base_resolver.clone().layer(unresolved.variables.clone());
    resolver = resolver.layer(extra_vars);

    let history_enabled = unresolved.history_enabled(collection.manifest.history);
    let request = unresolved.resolve(&resolver, base_dir)?;

    Ok(ResolvedRequest {
        request,
        history_enabled,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_format::CollectionManifest;
    use tempfile::tempdir;

    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn resolves_auth_inherited_from_an_ancestor_folder_toml() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "environments/dev.toml",
            "[variables]\ntoken = \"s3cr3t\"\n",
        );
        write(
            dir.path(),
            "folder.toml",
            "[auth]\ntype = \"bearer\"\ntoken = \"{{token}}\"\n",
        );
        write(
            dir.path(),
            "users/list.req.toml",
            "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"https://x.test/users\"\n",
        );

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        let resolved = load_and_resolve(
            &dir.path().join("users/list.req.toml"),
            &collection,
            Some("dev"),
            BTreeMap::new(),
        )
        .unwrap();

        let auth = resolved
            .request
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("authorization"))
            .unwrap();
        assert_eq!(auth.value, "Bearer s3cr3t");
    }

    #[test]
    fn respects_an_explicit_opt_out_of_folder_auth_inheritance() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "folder.toml",
            "[auth]\ntype = \"bearer\"\ntoken = \"s3cr3t\"\n",
        );
        write(
            dir.path(),
            "public.req.toml",
            "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"https://x.test\"\n\n[request.auth]\ntype = \"none\"\n",
        );

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        let resolved = load_and_resolve(
            &dir.path().join("public.req.toml"),
            &collection,
            None,
            BTreeMap::new(),
        )
        .unwrap();

        assert!(!resolved
            .request
            .headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("authorization")));
    }

    #[test]
    fn errors_when_a_variable_is_unresolved() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "a.req.toml",
            "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"https://{{missing}}\"\n",
        );

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        assert!(load_and_resolve(
            &dir.path().join("a.req.toml"),
            &collection,
            None,
            BTreeMap::new(),
        )
        .is_err());
    }

    #[test]
    fn interpolates_against_the_given_environment() {
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
        let resolved = load_and_resolve(
            &dir.path().join("a.req.toml"),
            &collection,
            Some("dev"),
            BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(resolved.request.url, "https://x.test");
    }

    #[test]
    fn history_enabled_by_default() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        RequestFile::create(&dir.path().join("a.req.toml"), "A", "GET", "https://x.test").unwrap();

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        let resolved = load_and_resolve(
            &dir.path().join("a.req.toml"),
            &collection,
            None,
            BTreeMap::new(),
        )
        .unwrap();
        assert!(resolved.history_enabled);
    }

    #[test]
    fn history_disabled_when_the_collection_disables_it() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "epistola.toml",
            "name = \"n\"\nhistory = false\n",
        );
        RequestFile::create(&dir.path().join("a.req.toml"), "A", "GET", "https://x.test").unwrap();

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        let resolved = load_and_resolve(
            &dir.path().join("a.req.toml"),
            &collection,
            None,
            BTreeMap::new(),
        )
        .unwrap();
        assert!(!resolved.history_enabled);
    }

    #[test]
    fn a_folder_can_re_enable_history_under_a_disabled_collection() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "epistola.toml",
            "name = \"n\"\nhistory = false\n",
        );
        write(dir.path(), "folder.toml", "history = true\n");
        RequestFile::create(&dir.path().join("a.req.toml"), "A", "GET", "https://x.test").unwrap();

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        let resolved = load_and_resolve(
            &dir.path().join("a.req.toml"),
            &collection,
            None,
            BTreeMap::new(),
        )
        .unwrap();
        assert!(resolved.history_enabled);
    }

    #[test]
    fn the_requests_own_history_field_wins_over_a_folder_override() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(dir.path(), "folder.toml", "history = true\n");
        write(
            dir.path(),
            "a.req.toml",
            "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"https://x.test\"\nhistory = false\n",
        );

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        let resolved = load_and_resolve(
            &dir.path().join("a.req.toml"),
            &collection,
            None,
            BTreeMap::new(),
        )
        .unwrap();
        assert!(!resolved.history_enabled);
    }

    #[test]
    fn extra_vars_take_precedence_over_the_requests_own_variables() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "a.req.toml",
            "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"https://x.test/{{page}}\"\n\n[request.variables]\npage = \"1\"\n",
        );

        let collection = LoadedCollection::discover_from(dir.path()).unwrap();
        let mut extra = BTreeMap::new();
        extra.insert("page".to_string(), "2".to_string());
        let resolved =
            load_and_resolve(&dir.path().join("a.req.toml"), &collection, None, extra).unwrap();
        assert_eq!(resolved.request.url, "https://x.test/2");
    }
}
