use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use epistola_engine::run::{execute_and_log, parse_var_overrides, resolve_saved_request};

use crate::client_config::ClientArgs;
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
    let vars = parse_var_overrides(&args.vars)?;
    let (collection, resolved) = resolve_saved_request(&args.path, args.env.as_deref(), vars)?;

    if args.verbose {
        eprint!("{}", output::format_request(&resolved.request));
    }

    let overrides = args.client.to_overrides();
    let outcome = execute_and_log(
        &resolved.request,
        resolved.history_enabled,
        &collection.root,
        &overrides,
        &collection.manifest.client,
        None,
    )
    .await?;
    let status = outcome.response.status;

    output::report_outcome(outcome, args.verbose, args.output.as_deref(), args.json)?;

    if args.check_status && status >= 400 {
        return Err(epistola_engine::EngineError::HttpStatusFailure(status).into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_engine::EngineError;
    use epistola_format::CollectionManifest;
    use tempfile::tempdir;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn write(dir: &std::path::Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn run_args(path: PathBuf) -> RunArgs {
        RunArgs {
            path,
            env: None,
            vars: Vec::new(),
            json: false,
            output: None,
            check_status: false,
            verbose: false,
            client: ClientArgs::default(),
        }
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

        let mut args = run_args(dir.path().join("a.req.toml"));
        args.verbose = true;
        run(args).await.unwrap();
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
        let mut args = run_args(dir.path().join("a.req.toml"));
        args.output = Some(output_path.clone());
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

        let mut args = run_args(dir.path().join("a.req.toml"));
        args.check_status = true;
        let err = run(args).await.unwrap_err();
        assert!(err.downcast_ref::<EngineError>().is_some());
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

        let mut args = run_args(dir.path().join("a.req.toml"));
        args.check_status = true;
        run(args).await.unwrap();
    }

    #[tokio::test]
    async fn json_flag_prints_the_response_as_json() {
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

        let mut args = run_args(dir.path().join("a.req.toml"));
        args.json = true;
        run(args).await.unwrap();
    }

    #[tokio::test]
    async fn errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "a.req.toml",
            "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"https://x.test\"\n",
        );

        let err = run(run_args(dir.path().join("a.req.toml")))
            .await
            .unwrap_err();
        assert!(err.downcast_ref::<EngineError>().is_some());
    }
}
