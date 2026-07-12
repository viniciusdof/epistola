use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Parser;
use epistola_core::HttpExecutor;
use epistola_format::{LoadedCollection, RequestFile};
use epistola_http::ReqwestExecutor;

use crate::cli::Cli;
use crate::commands::request::slugify;
use crate::output;

/// Ad-hoc, httpie-style request: `epistola GET <url> -H ... -q ... -d ...`
pub async fn run(args: Vec<String>, cwd: &Path) -> Result<()> {
    let cli = Cli::try_parse_from(std::iter::once("epistola".to_string()).chain(args))?;
    let save_as = cli.save.clone();
    let request = cli.into_request()?;

    // Fail fast on a bad --save before wasting a real network call.
    let save_path = match &save_as {
        Some(name) => Some(prepare_save_path(name, cwd)?),
        None => None,
    };

    let executor = ReqwestExecutor::new();
    let response = executor.execute(&request).await.context("request failed")?;

    print!("{}", output::format_response(&response));

    if let (Some(name), Some(path)) = (&save_as, &save_path) {
        RequestFile::from_request(name, &request)
            .create_at(path)
            .with_context(|| format!("failed to save request to '{}'", path.display()))?;
        println!("Saved to {}", path.display());
    }

    Ok(())
}

fn prepare_save_path(name: &str, cwd: &Path) -> Result<PathBuf> {
    let collection = LoadedCollection::discover_from(cwd).context(
        "--save requires being inside a collection (no epistola.toml found in this or any parent directory)",
    )?;
    let path = collection.root.join(format!("{}.req.toml", slugify(name)));
    if path.is_file() {
        bail!("'{}' already exists", path.display());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_format::CollectionManifest;
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

    #[test]
    fn prepare_save_path_errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        assert!(prepare_save_path("anything", dir.path()).is_err());
    }

    #[test]
    fn prepare_save_path_errors_on_an_existing_file() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        std::fs::write(dir.path().join("taken.req.toml"), "").unwrap();

        assert!(prepare_save_path("Taken", dir.path()).is_err());
    }
}
