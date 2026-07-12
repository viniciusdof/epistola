use std::collections::BTreeMap;
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use epistola_core::{Body, Header, Method, Request, VariableResolver};
use serde::{Deserialize, Serialize};

use crate::error::FormatError;
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
    #[serde(default, skip_serializing_if = "AuthSpec::is_none")]
    pub auth: AuthSpec,
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

/// `[request.auth]`, tagged on `type`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthSpec {
    #[default]
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
}

impl AuthSpec {
    fn is_none(&self) -> bool {
        matches!(self, AuthSpec::None)
    }
}

/// `[request.body]`, same tagged pattern.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BodySpec {
    #[default]
    None,
    Text {
        content: String,
    },
    Json {
        content: String,
    },
    Form {
        fields: Vec<FormField>,
    },
}

impl BodySpec {
    fn is_none(&self) -> bool {
        matches!(self, BodySpec::None)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FormField {
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
    pub auth: AuthSpec,
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
        let method = method
            .parse::<Method>()
            .unwrap_or_else(|never| match never {});
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
                    .map(|h| HeaderEntry {
                        name: h.name.clone(),
                        value: h.value.clone(),
                        disabled: false,
                    })
                    .collect(),
                auth: AuthSpec::None,
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

    /// Converts to an [`UnresolvedRequest`], dropping disabled headers/query.
    pub fn to_unresolved(&self) -> UnresolvedRequest {
        let method = self
            .request
            .method
            .parse::<Method>()
            .unwrap_or_else(|never| match never {});
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
            variables: self.request.variables.clone(),
        }
    }
}

impl UnresolvedRequest {
    /// Interpolates, encodes the body, and folds `auth` into an
    /// `Authorization` header. Never injects a default `Content-Type`.
    pub fn resolve(&self, resolver: &dyn VariableResolver) -> Result<Request, FormatError> {
        let mut request = epistola_core::interpolate_request(&self.request, resolver)?;

        request.body = match &self.body {
            BodySpec::None => Body::Empty,
            BodySpec::Text { content } | BodySpec::Json { content } => {
                Body::text(epistola_core::interpolate(content, resolver)?)
            }
            BodySpec::Form { fields } => {
                let mut serializer = form_urlencoded::Serializer::new(String::new());
                for field in fields.iter().filter(|f| !f.disabled) {
                    let value = epistola_core::interpolate(&field.value, resolver)?;
                    serializer.append_pair(&field.name, &value);
                }
                Body::text(serializer.finish())
            }
        };

        match &self.auth {
            AuthSpec::None => {}
            AuthSpec::Bearer { token } => {
                let token = epistola_core::interpolate(token, resolver)?;
                request
                    .headers
                    .push(Header::new("Authorization", format!("Bearer {token}")));
            }
            AuthSpec::Basic { username, password } => {
                let username = epistola_core::interpolate(username, resolver)?;
                let password = epistola_core::interpolate(password, resolver)?;
                let credentials = BASE64.encode(format!("{username}:{password}"));
                request
                    .headers
                    .push(Header::new("Authorization", format!("Basic {credentials}")));
            }
        }

        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

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
        assert!(matches!(file.request.auth, AuthSpec::None));
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
        assert!(matches!(file.request.auth, AuthSpec::Bearer { .. }));
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
            .resolve(&resolver(&[("tok", "abc")]))
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
            .resolve(&resolver(&[("user", "alice"), ("pass", "secret")]))
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
            .resolve(&resolver(&[("tok", "a b")]))
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
        let request = file.to_unresolved().resolve(&resolver(&[])).unwrap();
        assert!(!request
            .headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("content-type")));
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
        let result = file.to_unresolved().resolve(&resolver(&[]));
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
    fn load_reads_a_dot_epi_file_from_disk() {
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
}
