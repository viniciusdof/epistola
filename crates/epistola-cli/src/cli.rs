use std::path::Path;

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use epistola_core::{Body, LayeredVariableResolver, Method, Request};
use epistola_format::{BodySpec, MultipartPart};

use crate::client_config::ClientArgs;

/// A Rust-native HTTP client, built for the terminal.
#[derive(Parser, Debug)]
#[command(name = "epistola", version, about)]
pub struct Cli {
    /// HTTP method: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, QUERY, ...
    pub method: String,

    /// Request URL
    pub url: String,

    /// Header, as NAME:VALUE (repeatable)
    #[arg(short = 'H', long = "header", value_name = "NAME:VALUE")]
    pub headers: Vec<String>,

    /// Query parameter, as KEY=VALUE (repeatable)
    #[arg(short = 'q', long = "query", value_name = "KEY=VALUE")]
    pub query: Vec<String>,

    /// Request body, sent as-is
    #[arg(short = 'd', long = "data")]
    pub data: Option<String>,

    /// Multipart field, as NAME=VALUE or NAME=@PATH for a file (repeatable);
    /// mutually exclusive with -d/--data
    #[arg(short = 'F', long = "form", value_name = "NAME=VALUE|NAME=@PATH")]
    pub form: Vec<String>,

    /// Save this request into the current collection as `<NAME>.req.toml`
    #[arg(long)]
    pub save: Option<String>,

    #[command(flatten)]
    pub client: ClientArgs,
}

/// The ad-hoc CLI's body, before encoding. Narrower than `BodySpec` (only
/// what `-d`/`-F` can produce) so matching over it can be exhaustive with no
/// `unreachable!()` branch.
pub(crate) enum AdHocBody {
    None,
    Text(String),
    Multipart(Vec<MultipartPart>),
}

impl AdHocBody {
    pub(crate) fn into_body_spec(self) -> BodySpec {
        match self {
            AdHocBody::None => BodySpec::None,
            AdHocBody::Text(content) => BodySpec::Text { content },
            AdHocBody::Multipart(parts) => BodySpec::Multipart { parts },
        }
    }
}

fn parse_form_entry(raw: &str) -> Result<MultipartPart> {
    let (name, value) = raw
        .split_once('=')
        .ok_or_else(|| anyhow!("invalid -F '{raw}', expected NAME=VALUE or NAME=@PATH"))?;

    Ok(match value.strip_prefix('@') {
        Some(path) => MultipartPart::File {
            name: name.to_string(),
            path: path.to_string(),
            filename: None,
            content_type: None,
            disabled: false,
        },
        None => MultipartPart::Text {
            name: name.to_string(),
            value: value.to_string(),
            disabled: false,
        },
    })
}

impl Cli {
    pub(crate) fn ad_hoc_body(&self) -> Result<AdHocBody> {
        match (&self.data, self.form.is_empty()) {
            (Some(_), false) => bail!("-d/--data and -F/--form are mutually exclusive"),
            (Some(data), true) => Ok(AdHocBody::Text(data.clone())),
            (None, false) => {
                let parts = self
                    .form
                    .iter()
                    .map(|raw| parse_form_entry(raw))
                    .collect::<Result<Vec<_>>>()?;
                Ok(AdHocBody::Multipart(parts))
            }
            (None, true) => Ok(AdHocBody::None),
        }
    }

    /// Builds the send-ready `Request`. `cwd` anchors relative `-F
    /// NAME=@PATH` file paths — ad-hoc requests never interpolate `{{var}}`
    /// placeholders, so multipart parts are encoded with an empty resolver.
    pub fn into_request(self, cwd: &Path) -> Result<Request> {
        // Method::from_str is Infallible, so this can never actually fail.
        let method = self
            .method
            .parse::<Method>()
            .unwrap_or_else(|never| match never {});

        let body = self.ad_hoc_body()?;
        let mut request = Request::new(method, self.url);

        for raw in &self.headers {
            let (name, value) = raw
                .split_once(':')
                .ok_or_else(|| anyhow!("invalid header '{raw}', expected NAME:VALUE"))?;
            request = request.header(name.trim(), value.trim());
        }

        for raw in &self.query {
            let (key, value) = raw
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid query param '{raw}', expected KEY=VALUE"))?;
            request = request.query(key, value);
        }

        match body {
            AdHocBody::None => {}
            AdHocBody::Text(content) => request = request.body(Body::text(content)),
            AdHocBody::Multipart(parts) => {
                let boundary = epistola_format::generate_boundary();
                let resolver = LayeredVariableResolver::new();
                let bytes = epistola_format::encode_multipart(&parts, &resolver, cwd, &boundary)
                    .map_err(|err| anyhow!(err))?;
                request = request
                    .header(
                        "Content-Type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::Bytes(bytes));
            }
        }

        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("epistola").chain(args.iter().copied()))
    }

    #[test]
    fn builds_a_bare_get_request() {
        let request = cli(&["GET", "https://x.test"])
            .into_request(Path::new("."))
            .unwrap();
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.url, "https://x.test");
        assert!(request.headers.is_empty());
        assert!(request.query.is_empty());
        assert_eq!(request.body, Body::Empty);
    }

    #[test]
    fn parses_headers_and_trims_whitespace() {
        let request = cli(&["GET", "https://x.test", "-H", "X-Test: value"])
            .into_request(Path::new("."))
            .unwrap();
        assert_eq!(request.headers[0].name, "X-Test");
        assert_eq!(request.headers[0].value, "value");
    }

    #[test]
    fn rejects_a_header_without_a_colon() {
        let err = cli(&["GET", "https://x.test", "-H", "not-a-header"])
            .into_request(Path::new("."))
            .unwrap_err();
        assert!(err.to_string().contains("NAME:VALUE"));
    }

    #[test]
    fn parses_query_params() {
        let request = cli(&["GET", "https://x.test", "-q", "foo=bar"])
            .into_request(Path::new("."))
            .unwrap();
        assert_eq!(request.query, vec![("foo".to_string(), "bar".to_string())]);
    }

    #[test]
    fn rejects_a_query_param_without_an_equals_sign() {
        let err = cli(&["GET", "https://x.test", "-q", "foo"])
            .into_request(Path::new("."))
            .unwrap_err();
        assert!(err.to_string().contains("KEY=VALUE"));
    }

    #[test]
    fn data_flag_sets_the_body() {
        let request = cli(&["POST", "https://x.test", "-d", "payload"])
            .into_request(Path::new("."))
            .unwrap();
        assert_eq!(request.body, Body::text("payload"));
    }

    #[test]
    fn save_flag_defaults_to_none_and_parses_when_given() {
        assert_eq!(cli(&["GET", "https://x.test"]).save, None);
        assert_eq!(
            cli(&["GET", "https://x.test", "--save", "list-users"]).save,
            Some("list-users".to_string())
        );
    }

    #[test]
    fn form_flag_builds_a_multipart_body_with_a_text_field() {
        let request = cli(&["POST", "https://x.test", "-F", "caption=hi"])
            .into_request(Path::new("."))
            .unwrap();

        let content_type = &request
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("content-type"))
            .unwrap()
            .value;
        assert!(content_type.starts_with("multipart/form-data; boundary="));

        let body = String::from_utf8_lossy(request.body.as_bytes()).into_owned();
        assert!(body.contains("Content-Disposition: form-data; name=\"caption\"\r\n\r\nhi"));
    }

    #[test]
    fn form_flag_reads_a_file_relative_to_cwd() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("avatar.png"), b"pngbytes").unwrap();

        let request = cli(&["POST", "https://x.test", "-F", "file=@avatar.png"])
            .into_request(dir.path())
            .unwrap();

        let body = String::from_utf8_lossy(request.body.as_bytes()).into_owned();
        assert!(body.contains("filename=\"avatar.png\""));
        assert!(body.contains("pngbytes"));
    }

    #[test]
    fn data_and_form_together_is_an_error() {
        let err = cli(&["POST", "https://x.test", "-d", "x", "-F", "y=z"])
            .into_request(Path::new("."))
            .unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"));
    }
}
