use std::path::Path;

use anyhow::Result;
use clap::Parser;
use epistola_engine::adhoc;

use crate::cli::Cli;
use crate::output;

/// Ad-hoc, httpie-style request: `epistola GET <url> -H ... -q ... -d ...`
pub async fn run(args: Vec<String>, cwd: &Path) -> Result<()> {
    let cli = Cli::try_parse_from(std::iter::once("epistola".to_string()).chain(args))?;
    let save_as = cli.save.clone();
    let output_path = cli.output.clone();
    let check_status = cli.check_status;
    let verbose = cli.verbose;
    let overrides = cli.client.to_overrides();

    // Computed before `build_request` consumes the body; a `multipart` body
    // can't be reconstructed from its already-encoded bytes, so `--save`
    // needs the pre-encoding structure.
    let adhoc_request = cli.to_adhoc_request()?;
    let body_spec = adhoc_request.body.clone().into_body_spec();
    let request = adhoc::build_request(adhoc_request, cwd)?;

    // Fail fast on a bad --save before wasting a real network call.
    let save_path = match &save_as {
        Some(name) => Some(adhoc::prepare_save_path(name, cwd)?),
        None => None,
    };

    if verbose {
        eprint!("{}", output::format_request(&request));
    }

    let outcome = adhoc::run_adhoc_request(&request, cwd, &overrides).await?;
    let status = outcome.response.status;

    output::report_outcome(outcome, verbose, output_path.as_deref(), false)?;

    if let (Some(name), Some(path)) = (&save_as, &save_path) {
        adhoc::save_request(name, &request, body_spec, path)?;
        println!("Saved to {}", path.display());
    }

    if check_status && status >= 400 {
        return Err(epistola_engine::EngineError::HttpStatusFailure(status).into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_engine::EngineError;
    use epistola_format::{CollectionManifest, RequestFile};
    use tempfile::tempdir;
    use wiremock::matchers::{header, method, path as path_matcher, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn save_flag_persists_the_ad_hoc_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_matcher("/users"))
            .and(query_param("page", "1"))
            .and(header("x-test", "epistola"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        let args = vec![
            "GET".to_string(),
            format!("{}/users", server.uri()),
            "-q".to_string(),
            "page=1".to_string(),
            "-H".to_string(),
            "X-Test: epistola".to_string(),
            "--save".to_string(),
            "List Users".to_string(),
        ];
        run(args, dir.path()).await.unwrap();

        let saved_path = dir.path().join("list-users.req.toml");
        let file = RequestFile::load(&saved_path).unwrap();
        assert_eq!(file.request.name, "List Users");
        assert_eq!(file.request.query[0].value, "1");
        assert_eq!(file.request.headers[0].name, "X-Test");
    }

    #[tokio::test]
    async fn without_save_nothing_is_written_to_disk() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        run(vec!["GET".to_string(), server.uri()], dir.path())
            .await
            .unwrap();

        assert!(std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| {
                entry.path().extension().and_then(|e| e.to_str()) != Some("toml")
                    || entry.file_name() == "epistola.toml"
            }));
    }

    #[tokio::test]
    async fn verbose_flag_does_not_change_a_successful_ad_hoc_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        run(
            vec!["GET".to_string(), server.uri(), "-v".to_string()],
            dir.path(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn form_flag_sends_a_multipart_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_matcher("/upload"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        std::fs::write(dir.path().join("avatar.png"), b"pngbytes").unwrap();

        let args = vec![
            "POST".to_string(),
            format!("{}/upload", server.uri()),
            "-F".to_string(),
            "caption=hi".to_string(),
            "-F".to_string(),
            "avatar=@avatar.png".to_string(),
        ];
        run(args, dir.path()).await.unwrap();
    }

    #[tokio::test]
    async fn form_flag_with_save_writes_a_reloadable_multipart_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        std::fs::write(dir.path().join("avatar.png"), b"pngbytes").unwrap();

        let args = vec![
            "POST".to_string(),
            format!("{}/upload", server.uri()),
            "-F".to_string(),
            "caption=hi".to_string(),
            "-F".to_string(),
            "avatar=@avatar.png".to_string(),
            "--save".to_string(),
            "Upload avatar".to_string(),
        ];
        run(args, dir.path()).await.unwrap();

        let saved_path = dir.path().join("upload-avatar.req.toml");
        let file = RequestFile::load(&saved_path).unwrap();
        assert!(matches!(
            &file.request.body,
            epistola_format::BodySpec::Multipart { parts } if parts.len() == 2
        ));
    }

    #[tokio::test]
    async fn output_flag_writes_the_body_to_a_file() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"raw-bytes".to_vec()))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        let output_path = dir.path().join("out.bin");
        run(
            vec![
                "GET".to_string(),
                server.uri(),
                "--output".to_string(),
                output_path.to_string_lossy().into_owned(),
            ],
            dir.path(),
        )
        .await
        .unwrap();

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

        let err = run(
            vec![
                "GET".to_string(),
                server.uri(),
                "--check-status".to_string(),
            ],
            dir.path(),
        )
        .await
        .unwrap_err();
        assert!(err.downcast_ref::<EngineError>().is_some());
    }
}
