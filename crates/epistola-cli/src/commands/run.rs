use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::Args;
use epistola_core::HttpExecutor;
use epistola_format::{LoadedCollection, RequestFile};
use epistola_http::ReqwestExecutor;

use crate::output;

#[derive(Args, Debug)]
pub struct RunArgs {
    pub path: PathBuf,

    /// Environment to resolve against
    #[arg(long)]
    pub env: Option<String>,

    /// Extra variable, as KEY=VALUE (repeatable), highest precedence
    #[arg(long = "var", value_name = "KEY=VALUE")]
    pub vars: Vec<String>,

    #[arg(long)]
    pub json: bool,
}

pub async fn run(args: RunArgs) -> Result<()> {
    let file = RequestFile::load(&args.path)
        .with_context(|| format!("failed to load '{}'", args.path.display()))?;
    let collection = LoadedCollection::discover_from(&args.path).context(
        "not inside a collection (no epistola.toml found in this or any parent directory)",
    )?;

    let mut resolver = collection.resolver_for_environment(args.env.as_deref())?;
    let unresolved = file.to_unresolved();
    resolver = resolver.layer(unresolved.variables.clone());
    resolver = resolver.layer(parse_var_overrides(&args.vars)?);

    let request = unresolved.resolve(&resolver)?;

    let executor = ReqwestExecutor::new();
    let response = executor.execute(&request).await.context("request failed")?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output::response_to_json(&response))?
        );
    } else {
        print!("{}", output::format_response(&response));
    }

    Ok(())
}

fn parse_var_overrides(raw: &[String]) -> Result<BTreeMap<String, String>> {
    let mut vars = BTreeMap::new();
    for entry in raw {
        let (key, value) = entry
            .split_once('=')
            .ok_or_else(|| anyhow!("invalid --var '{entry}', expected KEY=VALUE"))?;
        vars.insert(key.to_string(), value.to_string());
    }
    Ok(vars)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_format::CollectionManifest;
    use tempfile::tempdir;
    use wiremock::matchers::{header, method, path as path_matcher, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn write(dir: &std::path::Path, rel: &str, content: &str) {
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

    #[tokio::test]
    async fn run_resolves_and_executes_a_saved_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_matcher("/users"))
            .and(query_param("greeting", "hi"))
            .and(header("authorization", "Bearer s3cr3t"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "environments/dev.toml",
            "[variables]\ntoken = \"s3cr3t\"\n",
        );
        write(
            dir.path(),
            "users/list.req.toml",
            &format!(
                "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"{}/users\"\n\n[[request.query]]\nname = \"greeting\"\nvalue = \"{{{{greeting}}}}\"\n\n[request.auth]\ntype = \"bearer\"\ntoken = \"{{{{token}}}}\"\n",
                server.uri()
            ),
        );

        let args = RunArgs {
            path: dir.path().join("users/list.req.toml"),
            env: Some("dev".to_string()),
            vars: vec!["greeting=hi".to_string()],
            json: false,
        };

        run(args).await.unwrap();
    }

    #[tokio::test]
    async fn run_errors_when_a_variable_is_unresolved() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "a.req.toml",
            "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"https://{{missing}}\"\n",
        );

        let args = RunArgs {
            path: dir.path().join("a.req.toml"),
            env: None,
            vars: Vec::new(),
            json: false,
        };

        assert!(run(args).await.is_err());
    }
}
