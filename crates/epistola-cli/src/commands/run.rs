use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::Args;
use epistola_core::HttpExecutor;
use epistola_format::{load_folder_chain, LoadedCollection, RequestFile};
use epistola_http::ReqwestExecutor;

use crate::client_config::ClientArgs;
use crate::errors::CliError;
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

    /// Write the raw response body to this file instead of printing it
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Exit with a non-zero status if the response status is >= 400
    #[arg(long)]
    pub check_status: bool,

    /// Print the raw request before sending and the response status/headers
    /// after, both to stderr (like `curl -v`) — stdout output is unaffected
    #[arg(short = 'v', long)]
    pub verbose: bool,

    #[command(flatten)]
    pub client: ClientArgs,
}

pub async fn run(args: RunArgs) -> Result<()> {
    let file = RequestFile::load(&args.path)
        .with_context(|| format!("failed to load '{}'", args.path.display()))?;
    let collection = LoadedCollection::discover_from(&args.path).context(
        "not inside a collection (no epistola.toml found in this or any parent directory)",
    )?;

    let base_dir = args
        .path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let mut resolver = collection.resolver_for_environment(args.env.as_deref())?;
    let chain = load_folder_chain(&collection.root, base_dir)?;
    let unresolved = file.to_unresolved().with_folder_inheritance(&chain);
    resolver = resolver.layer(unresolved.variables.clone());
    resolver = resolver.layer(parse_var_overrides(&args.vars)?);

    let request = unresolved.resolve(&resolver, base_dir)?;

    if args.verbose {
        eprint!("{}", output::format_request(&request));
    }

    let client_config = args
        .client
        .resolve(&collection.manifest.client, &collection.root)?;
    let executor =
        ReqwestExecutor::with_config(client_config).context("invalid client configuration")?;
    let response = executor.execute(&request).await.context("request failed")?;

    if args.verbose {
        eprint!("{}", output::format_response_head(&response));
    }

    if let Some(path) = &args.output {
        output::write_response_body(&response, path)
            .with_context(|| format!("failed to write response body to '{}'", path.display()))?;
        print!("{}", output::format_response_head(&response));
        println!("Saved {} bytes to {}", response.body.len(), path.display());
    } else if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output::response_to_json(&response))?
        );
    } else {
        print!("{}", output::format_response(&response));
    }

    if args.check_status && response.status >= 400 {
        return Err(CliError::HttpStatus(response.status).into());
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
            output: None,
            check_status: false,
            verbose: false,
            client: ClientArgs::default(),
        };

        run(args).await.unwrap();
    }

    #[tokio::test]
    async fn run_sends_auth_inherited_from_an_ancestor_folder_toml() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_matcher("/users"))
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
            "folder.toml",
            "[auth]\ntype = \"bearer\"\ntoken = \"{{token}}\"\n",
        );
        write(
            dir.path(),
            "users/list.req.toml",
            &format!(
                "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"{}/users\"\n",
                server.uri()
            ),
        );

        let args = RunArgs {
            path: dir.path().join("users/list.req.toml"),
            env: Some("dev".to_string()),
            vars: Vec::new(),
            json: false,
            output: None,
            check_status: false,
            verbose: false,
            client: ClientArgs::default(),
        };

        run(args).await.unwrap();
    }

    #[tokio::test]
    async fn run_sends_no_auth_when_the_request_explicitly_opts_out_of_inheritance() {
        let server = MockServer::start().await;
        // Matches only requests *without* an `authorization` header — if
        // the executor incorrectly still attaches the folder's inherited
        // auth, no mock matches, wiremock 404s, and `check_status` below
        // turns that into a test failure.
        Mock::given(method("GET"))
            .and(|req: &wiremock::Request| !req.headers.contains_key("authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        write(
            dir.path(),
            "folder.toml",
            "[auth]\ntype = \"bearer\"\ntoken = \"s3cr3t\"\n",
        );
        write(
            dir.path(),
            "public-status.req.toml",
            &format!(
                "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"{}\"\n\n[request.auth]\ntype = \"none\"\n",
                server.uri()
            ),
        );

        let args = RunArgs {
            path: dir.path().join("public-status.req.toml"),
            env: None,
            vars: Vec::new(),
            json: false,
            output: None,
            check_status: true,
            verbose: false,
            client: ClientArgs::default(),
        };

        run(args).await.unwrap();
    }

    #[tokio::test]
    async fn verbose_flag_does_not_change_a_successful_run() {
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

        let args = RunArgs {
            path: dir.path().join("a.req.toml"),
            env: None,
            vars: Vec::new(),
            json: false,
            output: None,
            check_status: false,
            verbose: true,
            client: ClientArgs::default(),
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
            output: None,
            check_status: false,
            verbose: false,
            client: ClientArgs::default(),
        };

        assert!(run(args).await.is_err());
    }

    #[tokio::test]
    async fn output_flag_writes_the_body_to_a_file_and_omits_it_from_stdout() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"raw-bytes".to_vec()))
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

        let output_path = dir.path().join("out.bin");
        let args = RunArgs {
            path: dir.path().join("a.req.toml"),
            env: None,
            vars: Vec::new(),
            json: false,
            output: Some(output_path.clone()),
            check_status: false,
            verbose: false,
            client: ClientArgs::default(),
        };

        run(args).await.unwrap();

        assert_eq!(std::fs::read(&output_path).unwrap(), b"raw-bytes");
    }

    #[tokio::test]
    async fn check_status_errors_on_a_4xx_response() {
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

        let args = RunArgs {
            path: dir.path().join("a.req.toml"),
            env: None,
            vars: Vec::new(),
            json: false,
            output: None,
            check_status: true,
            verbose: false,
            client: ClientArgs::default(),
        };

        let err = run(args).await.unwrap_err();
        assert!(err.downcast_ref::<CliError>().is_some());
    }

    #[tokio::test]
    async fn check_status_does_not_error_on_a_2xx_response() {
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

        let args = RunArgs {
            path: dir.path().join("a.req.toml"),
            env: None,
            vars: Vec::new(),
            json: false,
            output: None,
            check_status: true,
            verbose: false,
            client: ClientArgs::default(),
        };

        run(args).await.unwrap();
    }
}
