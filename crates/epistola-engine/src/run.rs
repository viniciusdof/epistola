use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use epistola_core::{HttpExecutor, Request, Response};
use epistola_format::{ClientSpec, LoadedCollection};
use epistola_http::{CookieJar, ReqwestExecutor};

use crate::client::{resolve_client_config, ClientOverrides};
use crate::discovery::discover_collection;
use crate::error::EngineError;
use crate::history;
use crate::resolve::{load_and_resolve, ResolvedRequest};

/// Parses repeated `KEY=VALUE` overrides (e.g. `--var`) into a map.
pub fn parse_var_overrides(raw: &[String]) -> Result<BTreeMap<String, String>, EngineError> {
    let mut vars = BTreeMap::new();
    for entry in raw {
        let (key, value) = entry
            .split_once('=')
            .ok_or_else(|| EngineError::InvalidVarOverride(entry.clone()))?;
        vars.insert(key.to_string(), value.to_string());
    }
    Ok(vars)
}

pub struct RunOutcome {
    pub response: Response,
    /// A history-append failure never fails the run, but it can't stay
    /// silent in a library either — the caller decides how to surface it.
    pub history_warning: Option<EngineError>,
}

/// Loads and resolves a saved request without sending it, so callers can
/// inspect/print the resolved request (e.g. `--verbose`) before it's sent.
/// Also returns the discovered collection, since [`execute_and_log`] needs
/// its root and `[client]` defaults.
pub fn resolve_saved_request(
    path: &Path,
    environment: Option<&str>,
    extra_vars: BTreeMap<String, String>,
) -> Result<(LoadedCollection, ResolvedRequest), EngineError> {
    let collection = discover_collection(path)?;
    let resolved = load_and_resolve(path, &collection, environment, extra_vars)?;
    Ok((collection, resolved))
}

/// Sends `request` and, if `history_enabled`, appends it to the
/// collection's history log. `cookie_jar` is `None` for one-shot callers.
pub async fn execute_and_log(
    request: &Request,
    history_enabled: bool,
    collection_root: &Path,
    overrides: &ClientOverrides,
    client_spec: &ClientSpec,
    cookie_jar: Option<Arc<CookieJar>>,
) -> Result<RunOutcome, EngineError> {
    let mut client_config = resolve_client_config(overrides, client_spec, collection_root)?;
    client_config.cookie_jar = cookie_jar;
    let executor = ReqwestExecutor::with_config(client_config)?;
    let response = executor.execute(request).await?;

    let history_warning = if history_enabled {
        history::append_entry(collection_root, request, &response).err()
    } else {
        None
    };

    Ok(RunOutcome {
        response,
        history_warning,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_format::CollectionManifest;
    use tempfile::tempdir;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn parse_var_overrides_builds_a_map() {
        let vars = parse_var_overrides(&["a=1".to_string(), "b=2".to_string()]).unwrap();
        assert_eq!(vars.get("a").map(String::as_str), Some("1"));
        assert_eq!(vars.get("b").map(String::as_str), Some("2"));
    }

    #[test]
    fn parse_var_overrides_rejects_an_entry_without_an_equals_sign() {
        let err = parse_var_overrides(&["nope".to_string()]).unwrap_err();
        assert!(err.to_string().contains("KEY=VALUE"));
    }

    async fn run_saved(path: std::path::PathBuf) -> RunOutcome {
        let (collection, resolved) = resolve_saved_request(&path, None, BTreeMap::new()).unwrap();
        execute_and_log(
            &resolved.request,
            resolved.history_enabled,
            &collection.root,
            &ClientOverrides::default(),
            &collection.manifest.client,
            None,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn resolves_and_executes_a_saved_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "a.req.toml",
            &format!(
                "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"{}\"\n",
                server.uri()
            ),
        );

        let outcome = run_saved(dir.path().join("a.req.toml")).await;
        assert_eq!(outcome.response.status, 200);
    }

    #[tokio::test]
    async fn appends_a_history_entry_by_default() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "a.req.toml",
            &format!(
                "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"{}\"\n",
                server.uri()
            ),
        );

        run_saved(dir.path().join("a.req.toml")).await;

        let entries = history::read_entries(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].response.status, 200);
    }

    #[tokio::test]
    async fn skips_history_when_resolution_says_disabled() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "epistola.toml",
            "name = \"n\"\nhistory = false\n",
        );
        write(
            dir.path(),
            "a.req.toml",
            &format!(
                "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"{}\"\n",
                server.uri()
            ),
        );

        run_saved(dir.path().join("a.req.toml")).await;

        assert!(!dir.path().join(".epistola").is_dir());
    }

    #[tokio::test]
    async fn still_logs_a_4xx_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "a.req.toml",
            &format!(
                "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"{}\"\n",
                server.uri()
            ),
        );

        let outcome = run_saved(dir.path().join("a.req.toml")).await;
        assert_eq!(outcome.response.status, 404);
        assert_eq!(history::read_entries(dir.path()).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn history_write_failure_is_reported_as_a_warning_not_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "a.req.toml",
            &format!(
                "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"{}\"\n",
                server.uri()
            ),
        );
        // A *file* named `.epistola` makes `create_dir_all` fail inside
        // `history::append_entry`.
        std::fs::write(dir.path().join(".epistola"), "not a directory").unwrap();

        let outcome = run_saved(dir.path().join("a.req.toml")).await;
        assert!(outcome.history_warning.is_some());
    }

    #[tokio::test]
    async fn a_shared_cookie_jar_persists_across_two_execute_and_log_calls() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/set"))
            .respond_with(ResponseTemplate::new(200).insert_header("Set-Cookie", "session=abc"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/echo"))
            .and(header("cookie", "session=abc"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        let request_for = |request_path: &str| {
            epistola_core::Request::get(format!("{}{request_path}", server.uri()))
        };

        let jar = std::sync::Arc::new(epistola_http::CookieJar::default());

        execute_and_log(
            &request_for("/set"),
            false,
            dir.path(),
            &ClientOverrides::default(),
            &epistola_format::ClientSpec::default(),
            Some(jar.clone()),
        )
        .await
        .unwrap();

        let outcome = execute_and_log(
            &request_for("/echo"),
            false,
            dir.path(),
            &ClientOverrides::default(),
            &epistola_format::ClientSpec::default(),
            Some(jar.clone()),
        )
        .await
        .unwrap();

        assert!(outcome.response.is_success());
    }
}
