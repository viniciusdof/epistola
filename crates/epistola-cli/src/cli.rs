use anyhow::{anyhow, Result};
use clap::Parser;
use epistola_core::{Body, Method, Request};

/// A Rust-native HTTP client, built for the terminal.
#[derive(Parser, Debug)]
#[command(name = "epistola", version, about)]
pub struct Cli {
    /// HTTP method: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, ...
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

    /// Save this request into the current collection as `<NAME>.req.toml`
    #[arg(long)]
    pub save: Option<String>,
}

impl Cli {
    pub fn into_request(self) -> Result<Request> {
        // Method::from_str is Infallible, so this can never actually fail.
        let method = self
            .method
            .parse::<Method>()
            .unwrap_or_else(|never| match never {});

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

        if let Some(data) = self.data {
            request = request.body(Body::text(data));
        }

        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("epistola").chain(args.iter().copied()))
    }

    #[test]
    fn builds_a_bare_get_request() {
        let request = cli(&["GET", "https://x.test"]).into_request().unwrap();
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.url, "https://x.test");
        assert!(request.headers.is_empty());
        assert!(request.query.is_empty());
        assert_eq!(request.body, Body::Empty);
    }

    #[test]
    fn parses_headers_and_trims_whitespace() {
        let request = cli(&["GET", "https://x.test", "-H", "X-Test: value"])
            .into_request()
            .unwrap();
        assert_eq!(request.headers[0].name, "X-Test");
        assert_eq!(request.headers[0].value, "value");
    }

    #[test]
    fn rejects_a_header_without_a_colon() {
        let err = cli(&["GET", "https://x.test", "-H", "not-a-header"])
            .into_request()
            .unwrap_err();
        assert!(err.to_string().contains("NAME:VALUE"));
    }

    #[test]
    fn parses_query_params() {
        let request = cli(&["GET", "https://x.test", "-q", "foo=bar"])
            .into_request()
            .unwrap();
        assert_eq!(request.query, vec![("foo".to_string(), "bar".to_string())]);
    }

    #[test]
    fn rejects_a_query_param_without_an_equals_sign() {
        let err = cli(&["GET", "https://x.test", "-q", "foo"])
            .into_request()
            .unwrap_err();
        assert!(err.to_string().contains("KEY=VALUE"));
    }

    #[test]
    fn data_flag_sets_the_body() {
        let request = cli(&["POST", "https://x.test", "-d", "payload"])
            .into_request()
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
}
