use std::path::{Path, PathBuf};

use epistola_core::{Body, LayeredVariableResolver, Method, Request};
use epistola_format::{load_folder_chain, BodySpec, LoadedCollection, MultipartPart, RequestFile};

use crate::client::ClientOverrides;
use crate::error::EngineError;
use crate::requests::slugified_request_path;
use crate::run::{execute_and_log, RunOutcome};

/// The ad-hoc request's body, before encoding. Narrower than `BodySpec`
/// (only what a `-d`/`-F`-style caller can produce) so matching over it can
/// be exhaustive with no `unreachable!()` branch.
#[derive(Debug, Clone)]
pub enum AdHocBody {
    None,
    Text(String),
    Multipart(Vec<MultipartPart>),
}

impl AdHocBody {
    pub fn into_body_spec(self) -> BodySpec {
        match self {
            AdHocBody::None => BodySpec::None,
            AdHocBody::Text(content) => BodySpec::Text { content },
            AdHocBody::Multipart(parts) => BodySpec::Multipart { parts },
        }
    }
}

/// A not-yet-built ad-hoc request — plain data, no CLI-syntax parsing
/// baked in, so any caller (CLI, GUI) can construct one.
#[derive(Debug)]
pub struct AdHocRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub body: AdHocBody,
}

/// Builds the send-ready `Request`. `cwd` anchors relative multipart file
/// paths — ad-hoc requests never interpolate `{{var}}` placeholders, so
/// multipart parts are encoded with an empty resolver.
pub fn build_request(request: AdHocRequest, cwd: &Path) -> Result<Request, EngineError> {
    let mut built = Request::new(request.method, request.url);

    for (name, value) in &request.headers {
        built = built.header(name, value);
    }
    for (key, value) in &request.query {
        built = built.query(key, value);
    }

    match request.body {
        AdHocBody::None => {}
        AdHocBody::Text(content) => built = built.body(Body::text(content)),
        AdHocBody::Multipart(parts) => {
            let boundary = epistola_format::generate_boundary();
            let resolver = LayeredVariableResolver::new();
            let bytes = epistola_format::encode_multipart(&parts, &resolver, cwd, &boundary)?;
            built = built
                .header(
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::Bytes(bytes));
        }
    }

    Ok(built)
}

/// Nearest-explicit-wins across `folder.toml`s above `cwd`, falling back to
/// the collection default, falling back to enabled.
fn history_enabled_for_adhoc(collection: &LoadedCollection, cwd: &Path) -> bool {
    let chain = load_folder_chain(&collection.root, cwd).unwrap_or_default();
    epistola_format::nearest_folder_history(&chain)
        .or(collection.manifest.history)
        .unwrap_or(true)
}

/// Sends an already-built ad-hoc request. Logs to history when run inside a
/// collection whose (folder-aware) history setting allows it; outside a
/// collection, never logs.
pub async fn run_adhoc_request(
    request: &Request,
    cwd: &Path,
    overrides: &ClientOverrides,
) -> Result<RunOutcome, EngineError> {
    let collection = LoadedCollection::discover_from(cwd).ok();

    let client_spec = collection
        .as_ref()
        .map(|c| c.manifest.client.clone())
        .unwrap_or_default();
    let base_dir = collection.as_ref().map(|c| c.root.as_path()).unwrap_or(cwd);
    let history_enabled = collection
        .as_ref()
        .is_some_and(|c| history_enabled_for_adhoc(c, cwd));

    execute_and_log(
        request,
        history_enabled,
        base_dir,
        overrides,
        &client_spec,
        None,
    )
    .await
}

/// Computes the path `--save NAME` would write to, erroring if it's outside
/// a collection or would overwrite an existing file.
pub fn prepare_save_path(name: &str, cwd: &Path) -> Result<PathBuf, EngineError> {
    let collection =
        LoadedCollection::discover_from(cwd).map_err(|source| EngineError::NotInCollection {
            message: "--save requires being inside a collection (no epistola.toml found in this or any parent directory)",
            source: Box::new(source),
        })?;
    let path = slugified_request_path(&collection.root, name);
    if path.is_file() {
        return Err(EngineError::AlreadyExists { path });
    }
    Ok(path)
}

pub fn save_request(
    name: &str,
    request: &Request,
    body: BodySpec,
    path: &Path,
) -> Result<(), EngineError> {
    RequestFile::from_request_and_body(name, request, body)
        .create_at(path)
        .map_err(EngineError::from)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_format::CollectionManifest;
    use tempfile::tempdir;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::history;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn get(url: String) -> AdHocRequest {
        AdHocRequest {
            method: Method::Get,
            url,
            headers: Vec::new(),
            query: Vec::new(),
            body: AdHocBody::None,
        }
    }

    #[test]
    fn build_request_sets_a_multipart_body_and_content_type() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("avatar.png"), b"pngbytes").unwrap();

        let request = build_request(
            AdHocRequest {
                method: Method::Post,
                url: "https://x.test/upload".to_string(),
                headers: Vec::new(),
                query: Vec::new(),
                body: AdHocBody::Multipart(vec![MultipartPart::File {
                    name: "avatar".to_string(),
                    path: "avatar.png".to_string(),
                    filename: None,
                    content_type: None,
                    disabled: false,
                }]),
            },
            dir.path(),
        )
        .unwrap();

        let content_type = &request
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("content-type"))
            .unwrap()
            .value;
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        let body = String::from_utf8_lossy(request.body.as_bytes()).into_owned();
        assert!(body.contains("pngbytes"));
    }

    #[tokio::test]
    async fn run_adhoc_request_appends_a_history_entry_inside_a_collection() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        let request = build_request(get(server.uri()), dir.path()).unwrap();
        run_adhoc_request(&request, dir.path(), &ClientOverrides::default())
            .await
            .unwrap();

        assert_eq!(history::read_entries(dir.path()).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn run_adhoc_request_skips_history_outside_a_collection() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        let request = build_request(get(server.uri()), dir.path()).unwrap();
        run_adhoc_request(&request, dir.path(), &ClientOverrides::default())
            .await
            .unwrap();

        assert!(!dir.path().join(".epistola").is_dir());
    }

    #[tokio::test]
    async fn run_adhoc_request_respects_the_collections_history_default() {
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

        let request = build_request(get(server.uri()), dir.path()).unwrap();
        run_adhoc_request(&request, dir.path(), &ClientOverrides::default())
            .await
            .unwrap();

        assert!(!dir.path().join(".epistola").is_dir());
    }

    #[tokio::test]
    async fn run_adhoc_request_respects_a_folder_tomls_history_override() {
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
        write(dir.path(), "folder.toml", "history = true\n");

        let request = build_request(get(server.uri()), dir.path()).unwrap();
        run_adhoc_request(&request, dir.path(), &ClientOverrides::default())
            .await
            .unwrap();

        assert_eq!(history::read_entries(dir.path()).unwrap().len(), 1);
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

    #[test]
    fn save_request_writes_a_reloadable_file() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();

        let request = build_request(get("https://x.test".to_string()), dir.path()).unwrap();
        let path = dir.path().join("saved.req.toml");
        save_request("Saved", &request, BodySpec::None, &path).unwrap();

        let file = RequestFile::load(&path).unwrap();
        assert_eq!(file.request.name, "Saved");
    }
}
