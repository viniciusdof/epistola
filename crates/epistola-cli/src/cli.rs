use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use epistola_engine::adhoc::{AdHocBody, AdHocRequest};
use epistola_format::MultipartPart;

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

/// Parses a `-F NAME=VALUE` or `-F NAME=@PATH` entry — httpie-style CLI
/// syntax, so this stays here rather than in `epistola_engine::adhoc`.
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

    /// Converts the parsed CLI args into the plain-data request
    /// `epistola_engine::adhoc` builds from — the only place httpie-style
    /// `NAME:VALUE`/`KEY=VALUE` string parsing happens.
    pub fn to_adhoc_request(&self) -> Result<AdHocRequest> {
        // Method::from_str is Infallible, so this can never actually fail.
        let method = self
            .method
            .parse::<epistola_core::Method>()
            .unwrap_or_else(|never| match never {});

        let mut headers = Vec::new();
        for raw in &self.headers {
            let (name, value) = raw
                .split_once(':')
                .ok_or_else(|| anyhow!("invalid header '{raw}', expected NAME:VALUE"))?;
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }

        let mut query = Vec::new();
        for raw in &self.query {
            let (key, value) = raw
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid query param '{raw}', expected KEY=VALUE"))?;
            query.push((key.to_string(), value.to_string()));
        }

        Ok(AdHocRequest {
            method,
            url: self.url.clone(),
            headers,
            query,
            body: self.ad_hoc_body()?,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::path::Path;

    use epistola_core::{Body, Method};
    use tempfile::tempdir;

    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("epistola").chain(args.iter().copied()))
    }

    fn built_request(args: &[&str], cwd: &Path) -> epistola_core::Request {
        let adhoc = cli(args).to_adhoc_request().unwrap();
        epistola_engine::adhoc::build_request(adhoc, cwd).unwrap()
    }

    #[test]
    fn builds_a_bare_get_request() {
        let request = built_request(&["GET", "https://x.test"], Path::new("."));
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.url, "https://x.test");
        assert!(request.headers.is_empty());
        assert!(request.query.is_empty());
        assert_eq!(request.body, Body::Empty);
    }

    #[test]
    fn parses_headers_and_trims_whitespace() {
        let request = built_request(
            &["GET", "https://x.test", "-H", "X-Test: value"],
            Path::new("."),
        );
        assert_eq!(request.headers[0].name, "X-Test");
        assert_eq!(request.headers[0].value, "value");
    }

    #[test]
    fn rejects_a_header_without_a_colon() {
        let err = cli(&["GET", "https://x.test", "-H", "not-a-header"])
            .to_adhoc_request()
            .unwrap_err();
        assert!(err.to_string().contains("NAME:VALUE"));
    }

    #[test]
    fn parses_query_params() {
        let request = built_request(&["GET", "https://x.test", "-q", "foo=bar"], Path::new("."));
        assert_eq!(request.query, vec![("foo".to_string(), "bar".to_string())]);
    }

    #[test]
    fn rejects_a_query_param_without_an_equals_sign() {
        let err = cli(&["GET", "https://x.test", "-q", "foo"])
            .to_adhoc_request()
            .unwrap_err();
        assert!(err.to_string().contains("KEY=VALUE"));
    }

    #[test]
    fn data_flag_sets_the_body() {
        let request = built_request(&["POST", "https://x.test", "-d", "payload"], Path::new("."));
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
        let request = built_request(
            &["POST", "https://x.test", "-F", "caption=hi"],
            Path::new("."),
        );

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

        let request = built_request(
            &["POST", "https://x.test", "-F", "file=@avatar.png"],
            dir.path(),
        );

        let body = String::from_utf8_lossy(request.body.as_bytes()).into_owned();
        assert!(body.contains("filename=\"avatar.png\""));
        assert!(body.contains("pngbytes"));
    }

    #[test]
    fn data_and_form_together_is_an_error() {
        let err = cli(&["POST", "https://x.test", "-d", "x", "-F", "y=z"])
            .to_adhoc_request()
            .unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"));
    }
}
