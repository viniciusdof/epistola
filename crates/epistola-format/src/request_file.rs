use std::collections::BTreeMap;
use std::path::Path;

use epistola_core::{Body, Header, Method, Request, VariableResolver};
use serde::{Deserialize, Serialize};

use crate::auth_spec::AuthSpec;
use crate::body_spec::BodySpec;
use crate::error::FormatError;
use crate::folder::FolderManifest;
use crate::toml_file::{read_toml_file, write_toml_file};

/// The full contents of a `.req.toml` request file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestFile {
    pub request: RequestSpec,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestSpec {
    pub name: String,
    pub method: String,
    pub url: String,
    /// Ordering within a folder; not consumed yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query: Vec<QueryEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<HeaderEntry>,
    /// `None` (the key absent) means "no opinion, inherit from the nearest
    /// ancestor `folder.toml`." `Some(AuthSpec::None)` (an explicit
    /// `type = "none"`) blocks inheritance and sends no auth. See
    /// [`UnresolvedRequest::with_folder_inheritance`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthSpec>,
    /// `None` (the key absent) means "no opinion, inherit from the nearest
    /// ancestor `folder.toml`, then the collection default." `Some(_)`
    /// wins outright, same rules as `auth`. See
    /// [`UnresolvedRequest::history_enabled`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<bool>,
    #[serde(default, skip_serializing_if = "BodySpec::is_none")]
    pub body: BodySpec,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeaderEntry {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryEntry {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}

/// A request whose `{{ var }}` placeholders haven't been substituted yet.
/// `body`/`auth` stay separate from `request` because they need
/// interpolation *before* encoding (base64, percent-encoding) — see
/// [`UnresolvedRequest::resolve`].
#[derive(Debug, Clone)]
pub struct UnresolvedRequest {
    pub request: Request,
    pub body: BodySpec,
    pub auth: Option<AuthSpec>,
    pub history: Option<bool>,
    pub variables: BTreeMap<String, String>,
}

impl RequestFile {
    pub fn from_toml_str(input: &str) -> Result<Self, FormatError> {
        Ok(toml::from_str(input)?)
    }

    pub fn load(path: &Path) -> Result<Self, FormatError> {
        read_toml_file(path)
    }

    /// Scaffolds a minimal `.req.toml` (name/method/url only).
    pub fn create(path: &Path, name: &str, method: &str, url: &str) -> Result<(), FormatError> {
        let method = Method::from(method);
        Self::from_request(name, &Request::new(method, url)).create_at(path)
    }

    /// Builds a `RequestFile` from a `Request` (e.g. for `--save`). A
    /// non-UTF-8 body is dropped rather than corrupted.
    pub fn from_request(name: &str, request: &Request) -> Self {
        let body = match &request.body {
            Body::Empty => BodySpec::None,
            Body::Bytes(bytes) => match std::str::from_utf8(bytes) {
                Ok(text) => BodySpec::Text {
                    content: text.to_string(),
                },
                Err(_) => BodySpec::None,
            },
        };
        Self::from_request_and_body(name, request, body)
    }

    /// Same as [`Self::from_request`], but with an explicit `body` instead
    /// of inferring one from `request.body`'s already-encoded bytes. Needed
    /// when the caller still has the pre-encoding structure on hand (e.g.
    /// the ad-hoc CLI's `-F` multipart parts), since a `multipart` body
    /// can't be reconstructed from its encoded bytes.
    ///
    /// For a `multipart` body, any `Content-Type` header on `request` is
    /// dropped: `resolve()` always regenerates that header itself (the
    /// boundary it names must match the boundary the body was just
    /// re-encoded with), so persisting the old one would save a header
    /// whose boundary silently goes stale the moment the file is re-run.
    pub fn from_request_and_body(name: &str, request: &Request, body: BodySpec) -> Self {
        let drop_content_type = matches!(body, BodySpec::Multipart { .. });
        RequestFile {
            request: RequestSpec {
                name: name.to_string(),
                method: request.method.to_string(),
                url: request.url.clone(),
                seq: None,
                query: request
                    .query
                    .iter()
                    .map(|(name, value)| QueryEntry {
                        name: name.clone(),
                        value: value.clone(),
                        disabled: false,
                    })
                    .collect(),
                headers: request
                    .headers
                    .iter()
                    .filter(|h| !(drop_content_type && h.name.eq_ignore_ascii_case("content-type")))
                    .map(|h| HeaderEntry {
                        name: h.name.clone(),
                        value: h.value.clone(),
                        disabled: false,
                    })
                    .collect(),
                auth: None,
                history: None,
                body,
                variables: BTreeMap::new(),
            },
        }
    }

    /// Writes `self` to `path`; errors if it already exists.
    pub fn create_at(&self, path: &Path) -> Result<(), FormatError> {
        if path.is_file() {
            return Err(FormatError::AlreadyExists {
                path: path.to_path_buf(),
            });
        }
        write_toml_file(path, self)
    }

    /// Overwrites `path` with `self`'s current contents.
    pub fn save_at(&self, path: &Path) -> Result<(), FormatError> {
        write_toml_file(path, self)
    }

    /// Converts to an [`UnresolvedRequest`], dropping disabled headers/query.
    pub fn to_unresolved(&self) -> UnresolvedRequest {
        let method = Method::from(self.request.method.as_str());
        let mut request = Request::new(method, &self.request.url);

        for q in self.request.query.iter().filter(|q| !q.disabled) {
            request = request.query(&q.name, &q.value);
        }
        for h in self.request.headers.iter().filter(|h| !h.disabled) {
            request = request.header(&h.name, &h.value);
        }

        UnresolvedRequest {
            request,
            body: self.request.body.clone(),
            auth: self.request.auth.clone(),
            history: self.request.history,
            variables: self.request.variables.clone(),
        }
    }
}

impl UnresolvedRequest {
    /// Merges inherited `folder.toml` headers/auth (root-to-leaf order in
    /// `chain`) into this request. Call between [`RequestFile::to_unresolved`]
    /// and [`Self::resolve`] — merging has to happen on the still-unresolved,
    /// uninterpolated values, same as everything else about `auth`/`body`.
    ///
    /// Headers: each level's (non-disabled) headers overwrite any
    /// same-name (case-insensitive) header from an earlier level, so a
    /// request's own headers always win over an inherited one, and an
    /// inherited header from a closer folder wins over a farther one.
    ///
    /// Auth: the request's own `auth` (including an explicit `none`)
    /// always wins outright. Only when the request has no opinion (`auth`
    /// is `None`) does this walk `chain` leaf-to-root for the nearest
    /// folder with an opinion.
    #[must_use]
    pub fn with_folder_inheritance(mut self, chain: &[FolderManifest]) -> Self {
        let mut headers: Vec<Header> = Vec::new();
        for folder in chain {
            for h in folder.headers.iter().filter(|h| !h.disabled) {
                upsert_header(&mut headers, &h.name, &h.value);
            }
        }
        for h in self.request.headers.drain(..) {
            upsert_header(&mut headers, &h.name, &h.value);
        }
        self.request.headers = headers;

        if self.auth.is_none() {
            self.auth = chain.iter().rev().find_map(|f| f.auth.clone());
        }

        if self.history.is_none() {
            self.history = crate::folder::nearest_folder_history(chain);
        }

        self
    }

    /// Resolves whether this execution should be appended to the
    /// collection's history log. Nearest-explicit-wins, same chain as
    /// `auth`: this request's own `history` (already folded with any
    /// `folder.toml` chain by [`Self::with_folder_inheritance`]) wins if
    /// set, else the collection manifest's `history`, else `true` —
    /// history logging is opt-out, not opt-in.
    pub fn history_enabled(&self, collection_default: Option<bool>) -> bool {
        self.history.or(collection_default).unwrap_or(true)
    }

    /// Interpolates, encodes the body, and folds `auth` into an
    /// `Authorization` header. Never injects a default `Content-Type` —
    /// except for `multipart`, whose `Content-Type` must carry the boundary
    /// actually used to encode the body, so it's mechanically required
    /// rather than guessed (same reasoning as the `Authorization` header
    /// below). Relative `file` part paths resolve against `base_dir`,
    /// normally the request file's own directory.
    pub fn resolve(
        &self,
        resolver: &dyn VariableResolver,
        base_dir: &std::path::Path,
    ) -> Result<Request, FormatError> {
        let mut request = epistola_core::interpolate_request(&self.request, resolver)?;

        let (body, content_type) = self.body.resolve(resolver, base_dir)?;
        request.body = body;
        if let Some(header) = content_type {
            request.headers.push(header);
        }

        let auth = self.auth.as_ref().unwrap_or(&AuthSpec::None);
        let (auth_headers, auth_query) = auth.resolve_headers_and_query(resolver)?;
        request.headers.extend(auth_headers);
        request.query.extend(auth_query);

        Ok(request)
    }
}

/// Inserts `name: value`, overwriting any existing entry with the same
/// name (case-insensitive) in place rather than appending a duplicate.
fn upsert_header(headers: &mut Vec<Header>, name: &str, value: &str) {
    match headers
        .iter_mut()
        .find(|h| h.name.eq_ignore_ascii_case(name))
    {
        Some(existing) => existing.value = value.to_string(),
        None => headers.push(Header::new(name, value)),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;
    use epistola_core::LayeredVariableResolver;
    use tempfile::tempdir;

    use super::*;

    fn resolver(pairs: &[(&str, &str)]) -> LayeredVariableResolver {
        LayeredVariableResolver::new().layer(BTreeMap::from_iter(
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())),
        ))
    }

    #[test]
    fn parses_a_minimal_get_request() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "List users"
            method = "GET"
            url = "https://x.test/users"
            "#,
        )
        .unwrap();
        assert_eq!(file.request.name, "List users");
        assert!(file.request.auth.is_none());
        assert!(matches!(file.request.body, BodySpec::None));
    }

    #[test]
    fn parses_headers_and_query_arrays_in_declared_order() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [[request.headers]]
            name = "A"
            value = "1"

            [[request.headers]]
            name = "B"
            value = "2"

            [[request.query]]
            name = "x"
            value = "1"

            [[request.query]]
            name = "y"
            value = "2"
            "#,
        )
        .unwrap();
        let unresolved = file.to_unresolved();
        assert_eq!(unresolved.request.headers[0].name, "A");
        assert_eq!(unresolved.request.headers[1].name, "B");
        assert_eq!(unresolved.request.query[0].0, "x");
        assert_eq!(unresolved.request.query[1].0, "y");
    }

    #[test]
    fn disabled_header_and_query_are_excluded_from_the_unresolved_request() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [[request.headers]]
            name = "X-Debug"
            value = "true"
            disabled = true

            [[request.query]]
            name = "page"
            value = "1"
            disabled = true
            "#,
        )
        .unwrap();
        let unresolved = file.to_unresolved();
        assert!(unresolved.request.headers.is_empty());
        assert!(unresolved.request.query.is_empty());
    }

    #[test]
    fn unknown_method_string_maps_to_method_other() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "PROPFIND"
            url = "https://x.test"
            "#,
        )
        .unwrap();
        let unresolved = file.to_unresolved();
        assert_eq!(unresolved.request.method.as_str(), "PROPFIND");
    }

    #[test]
    fn parses_bearer_auth() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [request.auth]
            type = "bearer"
            token = "{{auth_token}}"
            "#,
        )
        .unwrap();
        assert!(matches!(file.request.auth, Some(AuthSpec::Bearer { .. })));
    }

    #[test]
    fn parses_apikey_auth() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [request.auth]
            type = "apikey"
            location = "header"
            name = "X-Api-Key"
            value = "{{api_key}}"
            "#,
        )
        .unwrap();
        assert!(matches!(file.request.auth, Some(AuthSpec::ApiKey { .. })));
    }

    #[test]
    fn resolve_folds_a_header_apikey_into_a_custom_header() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [request.auth]
            type = "apikey"
            location = "header"
            name = "X-Api-Key"
            value = "{{key}}"
            "#,
        )
        .unwrap();
        let request = file
            .to_unresolved()
            .resolve(&resolver(&[("key", "s3cr3t")]), Path::new("."))
            .unwrap();
        assert_eq!(request.headers[0].name, "X-Api-Key");
        assert_eq!(request.headers[0].value, "s3cr3t");
    }

    #[test]
    fn resolve_folds_a_query_apikey_into_the_query_string() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [request.auth]
            type = "apikey"
            location = "query"
            name = "api_key"
            value = "{{key}}"
            "#,
        )
        .unwrap();
        let request = file
            .to_unresolved()
            .resolve(&resolver(&[("key", "s3cr3t")]), Path::new("."))
            .unwrap();
        assert_eq!(
            request.query,
            vec![("api_key".to_string(), "s3cr3t".to_string())]
        );
    }

    #[test]
    fn parses_text_body() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "POST"
            url = "https://x.test"

            [request.body]
            type = "text"
            content = "hello"
            "#,
        )
        .unwrap();
        assert!(matches!(file.request.body, BodySpec::Text { .. }));
    }

    #[test]
    fn resolve_folds_bearer_token_into_an_authorization_header() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [request.auth]
            type = "bearer"
            token = "{{tok}}"
            "#,
        )
        .unwrap();
        let request = file
            .to_unresolved()
            .resolve(&resolver(&[("tok", "abc")]), Path::new("."))
            .unwrap();
        assert_eq!(request.headers[0].value, "Bearer abc");
    }

    #[test]
    fn resolve_folds_basic_auth_into_a_base64_authorization_header() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [request.auth]
            type = "basic"
            username = "{{user}}"
            password = "{{pass}}"
            "#,
        )
        .unwrap();
        let request = file
            .to_unresolved()
            .resolve(
                &resolver(&[("user", "alice"), ("pass", "secret")]),
                Path::new("."),
            )
            .unwrap();
        assert_eq!(
            request.headers[0].value,
            format!("Basic {}", BASE64.encode("alice:secret"))
        );
    }

    #[test]
    fn resolve_interpolates_form_field_values_before_percent_encoding() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "POST"
            url = "https://x.test"

            [request.body]
            type = "form"

            [[request.body.fields]]
            name = "token"
            value = "{{tok}}"
            "#,
        )
        .unwrap();
        let request = file
            .to_unresolved()
            .resolve(&resolver(&[("tok", "a b")]), Path::new("."))
            .unwrap();
        assert_eq!(request.body, Body::text("token=a+b"));
    }

    #[test]
    fn json_body_does_not_inject_a_content_type_header() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "POST"
            url = "https://x.test"

            [request.body]
            type = "json"
            content = "{}"
            "#,
        )
        .unwrap();
        let request = file
            .to_unresolved()
            .resolve(&resolver(&[]), Path::new("."))
            .unwrap();
        assert!(!request
            .headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("content-type")));
    }

    #[test]
    fn parses_multipart_body() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "POST"
            url = "https://x.test"

            [request.body]
            type = "multipart"

            [[request.body.parts]]
            type = "text"
            name = "field"
            value = "value"

            [[request.body.parts]]
            type = "file"
            name = "avatar"
            path = "avatar.png"
            "#,
        )
        .unwrap();
        assert!(matches!(file.request.body, BodySpec::Multipart { .. }));
    }

    #[test]
    fn resolve_encodes_a_multipart_text_field_and_sets_the_boundary_content_type() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "POST"
            url = "https://x.test"

            [request.body]
            type = "multipart"

            [[request.body.parts]]
            type = "text"
            name = "field"
            value = "{{val}}"
            "#,
        )
        .unwrap();
        let request = file
            .to_unresolved()
            .resolve(&resolver(&[("val", "hello")]), Path::new("."))
            .unwrap();

        let content_type = &request
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("content-type"))
            .unwrap()
            .value;
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        let boundary = content_type
            .strip_prefix("multipart/form-data; boundary=")
            .unwrap();

        let body = std::str::from_utf8(request.body.as_bytes()).unwrap();
        assert!(body.contains(&format!("--{boundary}\r\n")));
        assert!(body.contains("Content-Disposition: form-data; name=\"field\"\r\n\r\nhello"));
        assert!(body.ends_with(&format!("--{boundary}--\r\n")));
    }

    #[test]
    fn resolve_reads_a_multipart_file_part_relative_to_base_dir() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("avatar.png"), b"pngbytes").unwrap();

        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "POST"
            url = "https://x.test"

            [request.body]
            type = "multipart"

            [[request.body.parts]]
            type = "file"
            name = "avatar"
            path = "avatar.png"
            content_type = "image/png"
            "#,
        )
        .unwrap();
        let request = file
            .to_unresolved()
            .resolve(&resolver(&[]), dir.path())
            .unwrap();

        let body = String::from_utf8_lossy(request.body.as_bytes()).into_owned();
        assert!(body.contains(
            "Content-Disposition: form-data; name=\"avatar\"; filename=\"avatar.png\"\r\n"
        ));
        assert!(body.contains("Content-Type: image/png\r\n"));
        assert!(body.contains("pngbytes"));
    }

    #[test]
    fn resolve_errors_when_a_multipart_file_part_is_missing() {
        let dir = tempdir().unwrap();
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "POST"
            url = "https://x.test"

            [request.body]
            type = "multipart"

            [[request.body.parts]]
            type = "file"
            name = "avatar"
            path = "missing.png"
            "#,
        )
        .unwrap();
        let result = file.to_unresolved().resolve(&resolver(&[]), dir.path());
        assert!(matches!(result, Err(FormatError::Io { .. })));
    }

    #[test]
    fn resolve_skips_a_disabled_multipart_part() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "POST"
            url = "https://x.test"

            [request.body]
            type = "multipart"

            [[request.body.parts]]
            type = "text"
            name = "field"
            value = "value"
            disabled = true
            "#,
        )
        .unwrap();
        let request = file
            .to_unresolved()
            .resolve(&resolver(&[]), Path::new("."))
            .unwrap();

        let body = std::str::from_utf8(request.body.as_bytes()).unwrap();
        assert!(!body.contains("field"));
    }

    #[test]
    fn resolve_propagates_an_unknown_variable_error() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://{{missing}}"
            "#,
        )
        .unwrap();
        let result = file.to_unresolved().resolve(&resolver(&[]), Path::new("."));
        assert!(matches!(result, Err(FormatError::Interpolation(_))));
    }

    #[test]
    fn rejects_an_unknown_auth_type() {
        let result = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [request.auth]
            type = "oauth2"
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn load_reads_a_req_toml_file_from_disk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("get-users.req.toml");
        std::fs::write(
            &path,
            "[request]\nname = \"n\"\nmethod = \"GET\"\nurl = \"https://x.test\"\n",
        )
        .unwrap();

        let file = RequestFile::load(&path).unwrap();
        assert_eq!(file.request.url, "https://x.test");
    }

    #[test]
    fn load_errors_with_path_context_when_the_file_is_missing() {
        let dir = tempdir().unwrap();
        let err = RequestFile::load(&dir.path().join("nope.req.toml")).unwrap_err();
        assert!(matches!(err, FormatError::Io { .. }));
    }

    #[test]
    fn create_writes_a_loadable_request_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("list-users.req.toml");
        RequestFile::create(&path, "List users", "GET", "https://x.test/users").unwrap();

        let file = RequestFile::load(&path).unwrap();
        assert_eq!(file.request.name, "List users");
        assert_eq!(file.request.method, "GET");
        assert_eq!(file.request.url, "https://x.test/users");
        assert!(file.request.headers.is_empty());
    }

    #[test]
    fn create_refuses_to_overwrite_an_existing_request_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("list-users.req.toml");
        RequestFile::create(&path, "First", "GET", "https://x.test").unwrap();

        let err = RequestFile::create(&path, "Second", "POST", "https://y.test").unwrap_err();
        assert!(matches!(err, FormatError::AlreadyExists { .. }));
        assert_eq!(RequestFile::load(&path).unwrap().request.name, "First");
    }

    #[test]
    fn from_request_captures_method_url_headers_query_and_text_body() {
        let request = Request::new(Method::Post, "https://x.test/users")
            .header("Authorization", "Bearer abc")
            .query("page", "1")
            .body(Body::text("payload"));

        let file = RequestFile::from_request("Ad-hoc", &request);

        assert_eq!(file.request.name, "Ad-hoc");
        assert_eq!(file.request.method, "POST");
        assert_eq!(file.request.url, "https://x.test/users");
        assert_eq!(file.request.headers[0].name, "Authorization");
        assert_eq!(file.request.headers[0].value, "Bearer abc");
        assert_eq!(file.request.query[0].name, "page");
        assert!(
            matches!(file.request.body, BodySpec::Text { ref content } if content == "payload")
        );
    }

    #[test]
    fn from_request_drops_a_non_utf8_body_instead_of_corrupting_it() {
        let request = Request::post("https://x.test").body(Body::Bytes(vec![0xff, 0xfe]));
        let file = RequestFile::from_request("n", &request);
        assert!(matches!(file.request.body, BodySpec::None));
    }

    #[test]
    fn from_request_and_body_drops_a_stale_multipart_content_type_header() {
        // Simulates the ad-hoc CLI: `into_request` already encoded the body
        // and injected a `Content-Type: multipart/form-data; boundary=X`
        // header before `--save` calls `from_request_and_body`. That header
        // must not survive into the saved file, since `resolve()` always
        // regenerates it with a fresh boundary on every run.
        let request = Request::new(Method::Post, "https://x.test/upload")
            .header(
                "Content-Type",
                "multipart/form-data; boundary=stale-boundary",
            )
            .header("X-Other", "kept")
            .body(Body::Bytes(b"...".to_vec()));

        let file = RequestFile::from_request_and_body(
            "Upload",
            &request,
            BodySpec::Multipart { parts: Vec::new() },
        );

        assert!(!file
            .request
            .headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("content-type")));
        assert_eq!(file.request.headers[0].name, "X-Other");
    }

    #[test]
    fn from_request_and_body_keeps_content_type_for_a_non_multipart_body() {
        let request = Request::new(Method::Post, "https://x.test")
            .header("Content-Type", "application/json")
            .body(Body::Bytes(b"{}".to_vec()));

        let file = RequestFile::from_request_and_body(
            "n",
            &request,
            BodySpec::Text {
                content: "{}".to_string(),
            },
        );

        assert_eq!(file.request.headers[0].name, "Content-Type");
    }

    #[test]
    fn create_at_writes_a_loadable_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ad-hoc.req.toml");
        let request = Request::get("https://x.test");

        RequestFile::from_request("Ad-hoc", &request)
            .create_at(&path)
            .unwrap();

        let loaded = RequestFile::load(&path).unwrap();
        assert_eq!(loaded.request.name, "Ad-hoc");
    }

    #[test]
    fn create_at_refuses_to_overwrite_an_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ad-hoc.req.toml");
        let request = Request::get("https://x.test");
        RequestFile::from_request("First", &request)
            .create_at(&path)
            .unwrap();

        let err = RequestFile::from_request("Second", &request)
            .create_at(&path)
            .unwrap_err();
        assert!(matches!(err, FormatError::AlreadyExists { .. }));
    }

    #[test]
    fn save_at_overwrites_an_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ad-hoc.req.toml");
        let request = Request::get("https://x.test");
        RequestFile::from_request("First", &request)
            .create_at(&path)
            .unwrap();

        RequestFile::from_request("Second", &request)
            .save_at(&path)
            .unwrap();

        let loaded = RequestFile::load(&path).unwrap();
        assert_eq!(loaded.request.name, "Second");
    }

    fn folder_with_header(name: &str, value: &str) -> FolderManifest {
        FolderManifest {
            headers: vec![HeaderEntry {
                name: name.to_string(),
                value: value.to_string(),
                disabled: false,
            }],
            auth: None,
            history: None,
        }
    }

    #[test]
    fn with_folder_inheritance_adds_headers_the_request_did_not_set() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"
            "#,
        )
        .unwrap();
        let chain = vec![folder_with_header("X-Api-Version", "1")];
        let unresolved = file.to_unresolved().with_folder_inheritance(&chain);

        assert_eq!(unresolved.request.headers[0].name, "X-Api-Version");
        assert_eq!(unresolved.request.headers[0].value, "1");
    }

    #[test]
    fn with_folder_inheritance_lets_the_requests_own_header_win_by_name() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [[request.headers]]
            name = "x-api-version"
            value = "2"
            "#,
        )
        .unwrap();
        let chain = vec![folder_with_header("X-Api-Version", "1")];
        let unresolved = file.to_unresolved().with_folder_inheritance(&chain);

        assert_eq!(unresolved.request.headers.len(), 1);
        assert_eq!(unresolved.request.headers[0].value, "2");
    }

    #[test]
    fn with_folder_inheritance_lets_a_closer_folder_win_over_a_farther_one() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"
            "#,
        )
        .unwrap();
        let chain = vec![
            folder_with_header("X-Api-Version", "root"),
            folder_with_header("X-Api-Version", "nested"),
        ];
        let unresolved = file.to_unresolved().with_folder_inheritance(&chain);

        assert_eq!(unresolved.request.headers.len(), 1);
        assert_eq!(unresolved.request.headers[0].value, "nested");
    }

    #[test]
    fn with_folder_inheritance_picks_the_nearest_ancestor_auth_when_the_request_has_none() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"
            "#,
        )
        .unwrap();
        let chain = vec![
            FolderManifest {
                headers: Vec::new(),
                auth: Some(AuthSpec::Bearer {
                    token: "{{root_token}}".to_string(),
                }),
                history: None,
            },
            FolderManifest {
                headers: Vec::new(),
                auth: Some(AuthSpec::Bearer {
                    token: "{{nested_token}}".to_string(),
                }),
                history: None,
            },
        ];
        let unresolved = file.to_unresolved().with_folder_inheritance(&chain);

        assert!(
            matches!(unresolved.auth, Some(AuthSpec::Bearer { ref token }) if token == "{{nested_token}}")
        );
    }

    #[test]
    fn with_folder_inheritance_leaves_the_requests_own_auth_untouched() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [request.auth]
            type = "bearer"
            token = "{{own_token}}"
            "#,
        )
        .unwrap();
        let chain = vec![FolderManifest {
            headers: Vec::new(),
            auth: Some(AuthSpec::Bearer {
                token: "{{folder_token}}".to_string(),
            }),
            history: None,
        }];
        let unresolved = file.to_unresolved().with_folder_inheritance(&chain);

        assert!(
            matches!(unresolved.auth, Some(AuthSpec::Bearer { ref token }) if token == "{{own_token}}")
        );
    }

    #[test]
    fn with_folder_inheritance_honors_an_explicit_none_and_blocks_inheritance() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [request.auth]
            type = "none"
            "#,
        )
        .unwrap();
        let chain = vec![FolderManifest {
            headers: Vec::new(),
            auth: Some(AuthSpec::Bearer {
                token: "{{token}}".to_string(),
            }),
            history: None,
        }];
        let unresolved = file.to_unresolved().with_folder_inheritance(&chain);
        let request = unresolved.resolve(&resolver(&[]), Path::new(".")).unwrap();

        assert!(!request
            .headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("authorization")));
    }

    #[test]
    fn with_folder_inheritance_is_a_noop_with_an_empty_chain() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"

            [[request.headers]]
            name = "X-Own"
            value = "1"
            "#,
        )
        .unwrap();
        let unresolved = file.to_unresolved().with_folder_inheritance(&[]);

        assert_eq!(unresolved.request.headers.len(), 1);
        assert_eq!(unresolved.request.headers[0].name, "X-Own");
        assert!(unresolved.auth.is_none());
    }

    #[test]
    fn parses_an_explicit_history_field_on_a_request() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"
            history = false
            "#,
        )
        .unwrap();

        assert_eq!(file.request.history, Some(false));
    }

    #[test]
    fn history_enabled_defaults_to_true_with_nothing_configured() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"
            "#,
        )
        .unwrap();
        let unresolved = file.to_unresolved().with_folder_inheritance(&[]);

        assert!(unresolved.history_enabled(None));
    }

    #[test]
    fn history_enabled_honors_an_explicit_collection_level_false() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"
            "#,
        )
        .unwrap();
        let unresolved = file.to_unresolved().with_folder_inheritance(&[]);

        assert!(!unresolved.history_enabled(Some(false)));
    }

    #[test]
    fn with_folder_inheritance_picks_the_nearest_ancestor_history_override() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"
            "#,
        )
        .unwrap();
        let chain = vec![
            FolderManifest {
                headers: Vec::new(),
                auth: None,
                history: Some(true),
            },
            FolderManifest {
                headers: Vec::new(),
                auth: None,
                history: Some(false),
            },
        ];
        let unresolved = file.to_unresolved().with_folder_inheritance(&chain);

        assert!(!unresolved.history_enabled(Some(true)));
    }

    #[test]
    fn with_folder_inheritance_leaves_the_requests_own_history_untouched() {
        let file = RequestFile::from_toml_str(
            r#"
            [request]
            name = "n"
            method = "GET"
            url = "https://x.test"
            history = true
            "#,
        )
        .unwrap();
        let chain = vec![FolderManifest {
            headers: Vec::new(),
            auth: None,
            history: Some(false),
        }];
        let unresolved = file.to_unresolved().with_folder_inheritance(&chain);

        assert!(unresolved.history_enabled(Some(false)));
    }
}
