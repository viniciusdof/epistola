//! reqwest-based `HttpExecutor` for epistola — a thin adapter between
//! `epistola_core::Request` and `reqwest::Request`.

use std::time::{Duration, Instant};

use epistola_core::{ExecutorError, Header, HttpExecutor, Request, Response};

/// Failure building a `reqwest::Client` (invalid proxy URL, malformed PEM
/// identity, ...). Distinct from `ExecutorError`, which covers failures
/// *executing* a request against an already-built client.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct BuildError(#[from] reqwest::Error);

#[derive(Debug, Clone, Default)]
pub struct ReqwestExecutor {
    client: reqwest::Client,
}

/// Client-wide execution behavior: timeout, redirect policy, proxy. Kept
/// separate from `epistola_core::Request`, which deliberately excludes
/// execution state.
#[derive(Debug, Clone, Default)]
pub struct ClientConfig {
    pub timeout: Option<Duration>,
    /// Max redirects to follow; `Some(0)` disables following redirects,
    /// `None` uses reqwest's own default (currently a limit of 10).
    pub max_redirects: Option<usize>,
    pub proxy: ProxyConfig,
    /// Skip TLS certificate validation entirely. Dangerous — only for hosts
    /// you trust (e.g. local dev servers with self-signed certs).
    pub insecure: bool,
    /// PEM bytes containing both a client certificate chain and its private
    /// key, for mutual-TLS. Combined-file only: this crate builds against
    /// reqwest's rustls backend, which only exposes `Identity::from_pem`
    /// (the native-tls-only separate-file constructors aren't available).
    pub client_identity_pem: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub enum ProxyConfig {
    /// Whatever reqwest does by default — honors `HTTP_PROXY`/`HTTPS_PROXY`.
    #[default]
    SystemDefault,
    /// Only `http(s)://` proxy URLs are supported; SOCKS would need
    /// reqwest's `"socks"` feature, which isn't enabled.
    Custom(String),
    Disabled,
}

impl ReqwestExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub fn with_config(config: ClientConfig) -> Result<Self, BuildError> {
        let mut builder = reqwest::Client::builder();

        if let Some(timeout) = config.timeout {
            builder = builder.timeout(timeout);
        }

        builder = match config.max_redirects {
            Some(0) => builder.redirect(reqwest::redirect::Policy::none()),
            Some(n) => builder.redirect(reqwest::redirect::Policy::limited(n)),
            None => builder,
        };

        builder = match &config.proxy {
            ProxyConfig::SystemDefault => builder,
            ProxyConfig::Custom(url) => builder.proxy(reqwest::Proxy::all(url)?),
            ProxyConfig::Disabled => builder.no_proxy(),
        };

        if config.insecure {
            builder = builder.tls_danger_accept_invalid_certs(true);
        }

        if let Some(pem) = &config.client_identity_pem {
            builder = builder.identity(reqwest::Identity::from_pem(pem)?);
        }

        Ok(Self {
            client: builder.build()?,
        })
    }
}

/// Maps a transport-level `reqwest::Error` into an `ExecutorError`, singling
/// out timeouts (reqwest's `timeout()` covers connect + request + response
/// read as a whole, so this applies at every `.await` point on the request).
fn map_transport_error(err: reqwest::Error, elapsed: Duration) -> ExecutorError {
    if err.is_timeout() {
        ExecutorError::Timeout(elapsed)
    } else {
        ExecutorError::Transport(Box::new(err))
    }
}

impl HttpExecutor for ReqwestExecutor {
    async fn execute(&self, request: &Request) -> Result<Response, ExecutorError> {
        let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
            .map_err(|err| ExecutorError::InvalidRequest(err.to_string()))?;

        let mut builder = self
            .client
            .request(method, &request.url)
            .query(&request.query);

        for header in &request.headers {
            builder = builder.header(&header.name, &header.value);
        }

        if !request.body.is_empty() {
            builder = builder.body(request.body.as_bytes().to_vec());
        }

        let start = Instant::now();
        let response = builder
            .send()
            .await
            .map_err(|err| map_transport_error(err, start.elapsed()))?;

        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                Header::new(name.to_string(), value.to_str().unwrap_or("").to_string())
            })
            .collect();

        let body = response
            .bytes()
            .await
            .map_err(|err| map_transport_error(err, start.elapsed()))?
            .to_vec();
        let duration = start.elapsed();

        Ok(Response {
            status,
            headers,
            body,
            duration,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_core::{Body, Method};
    use wiremock::matchers::{body_bytes, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn executes_a_get_request_and_returns_status_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/hello"))
            .respond_with(ResponseTemplate::new(200).set_body_string("world"))
            .mount(&server)
            .await;

        let response = ReqwestExecutor::new()
            .execute(&Request::get(format!("{}/hello", server.uri())))
            .await
            .unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(response.body_as_str().unwrap(), "world");
    }

    #[tokio::test]
    async fn sends_query_params_and_custom_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("q", "rust"))
            .and(header("x-test", "epistola"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let request = Request::get(format!("{}/search", server.uri()))
            .query("q", "rust")
            .header("X-Test", "epistola");

        let response = ReqwestExecutor::new().execute(&request).await.unwrap();

        assert!(response.is_success());
    }

    #[tokio::test]
    async fn sends_the_request_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/echo"))
            .and(body_bytes(b"payload".to_vec()))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        let request = Request::post(format!("{}/echo", server.uri())).body(Body::text("payload"));

        let response = ReqwestExecutor::new().execute(&request).await.unwrap();

        assert_eq!(response.status, 201);
    }

    #[tokio::test]
    async fn invalid_url_surfaces_as_a_transport_error() {
        let result = ReqwestExecutor::new()
            .execute(&Request::get("not a valid url"))
            .await;

        assert!(matches!(result, Err(ExecutorError::Transport(_))));
    }

    #[tokio::test]
    async fn method_with_invalid_token_characters_surfaces_as_invalid_request() {
        let request = Request::new(Method::Other("BAD METHOD".to_string()), "https://x.test");

        let result = ReqwestExecutor::new().execute(&request).await;

        assert!(matches!(result, Err(ExecutorError::InvalidRequest(_))));
    }

    #[tokio::test]
    async fn with_client_uses_the_given_reqwest_client() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/hello"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let executor = ReqwestExecutor::with_client(reqwest::Client::new());
        let response = executor
            .execute(&Request::get(format!("{}/hello", server.uri())))
            .await
            .unwrap();

        assert_eq!(response.status, 204);
    }

    #[tokio::test]
    async fn with_config_timeout_surfaces_as_a_timeout_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(200)))
            .mount(&server)
            .await;

        let executor = ReqwestExecutor::with_config(ClientConfig {
            timeout: Some(Duration::from_millis(20)),
            ..Default::default()
        })
        .unwrap();

        let result = executor
            .execute(&Request::get(format!("{}/slow", server.uri())))
            .await;

        assert!(matches!(result, Err(ExecutorError::Timeout(_))));
    }

    #[tokio::test]
    async fn with_config_zero_max_redirects_does_not_follow() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/start"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", "/target"))
            .mount(&server)
            .await;

        let executor = ReqwestExecutor::with_config(ClientConfig {
            max_redirects: Some(0),
            ..Default::default()
        })
        .unwrap();

        let response = executor
            .execute(&Request::get(format!("{}/start", server.uri())))
            .await
            .unwrap();

        assert_eq!(response.status, 302);
    }

    #[tokio::test]
    async fn with_config_default_follows_redirects() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/start"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", "/target"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/target"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let executor = ReqwestExecutor::with_config(ClientConfig::default()).unwrap();

        let response = executor
            .execute(&Request::get(format!("{}/start", server.uri())))
            .await
            .unwrap();

        assert_eq!(response.status, 200);
    }

    #[test]
    fn with_config_builds_successfully_with_a_custom_proxy() {
        let result = ReqwestExecutor::with_config(ClientConfig {
            proxy: ProxyConfig::Custom("http://127.0.0.1:9".to_string()),
            ..Default::default()
        });

        assert!(result.is_ok());
    }

    #[test]
    fn with_config_builds_successfully_when_insecure_is_set() {
        let result = ReqwestExecutor::with_config(ClientConfig {
            insecure: true,
            ..Default::default()
        });

        assert!(result.is_ok());
    }

    // Self-signed, generated once via:
    // openssl req -x509 -newkey rsa:2048 -nodes -keyout - -out - -days 3650 -subj "/CN=epistola-test"
    // (cert to stdout, then key, concatenated). Test fixture only — not used
    // to secure anything.
    const TEST_IDENTITY_PEM: &str = concat!(
        "-----BEGIN CERTIFICATE-----\n",
        "MIIC1jCCAb6gAwIBAgIJANdo2oi+i8KWMA0GCSqGSIb3DQEBCwUAMBgxFjAUBgNV\n",
        "BAMMDWVwaXN0b2xhLXRlc3QwHhcNMjYwNzEzMDEwMjA5WhcNMzYwNzEwMDEwMjA5\n",
        "WjAYMRYwFAYDVQQDDA1lcGlzdG9sYS10ZXN0MIIBIjANBgkqhkiG9w0BAQEFAAOC\n",
        "AQ8AMIIBCgKCAQEA5fYdbT6b+thMKz/zpN5qDa2HAt6ErRNMvQKOHV2oSmM0qiFh\n",
        "zWid3r7X/oYZ2HQZ5rdnDqpMZ9w9FvfcN+G92SGcy/k3ZbsZDlj3wW49rNpIHIgB\n",
        "ktegbdDmrFIyW+vsJAUKCnfloU/Ij4xeY4Dfro710PiKJSbDaU+MnI5JC45VkKSw\n",
        "ytuQ7QiUvlirgV+yiFb4LR2hL8vWSvXLpjGp+jh0Z5bH940gkNdEKF+05PUKHbem\n",
        "XGKu4lQKpoBVbqquSKHzX1cwmxV6nqGvZ96ZvOhEx3B+KV0DksdSfYpvWdGto3nI\n",
        "rLf7G5/a5DPuW5ERx1rQx3VN7BQ6w5LvlkFffQIDAQABoyMwITAPBgNVHRMBAf8E\n",
        "BTADAQH/MA4GA1UdDwEB/wQEAwICpDANBgkqhkiG9w0BAQsFAAOCAQEAT/C9+6m9\n",
        "LcKHp9KIjSakgewJ08TJNAvf3CWBHQs/euVnZgWWlUDQ9wePQ+AfujYeNcjxuxm4\n",
        "jxHWAqYu+YcJVczLTGMrfnXH1i10H33gpTiN6tmq7pUAL2f0hLQOnSZPEKLVffuR\n",
        "aqys+p/J4aL8GPvGmE874+MdCcKBjT8plTh3legLH3r81mMzabrmh9F/wfNrIJ5A\n",
        "4e689E9GFFrtYXrPRaLoL4tS4QzK5FyjmV1P1fDPQBpIjhkJmCACXEDfSiSiADLK\n",
        "FBE4F0W8ElEhq8amANiHrG7hZk7AewsiZgwCKzom0eWGghD4kSiI7D/zAqh416xi\n",
        "5TlnOVamzKm32A==\n",
        "-----END CERTIFICATE-----\n",
        "-----BEGIN PRIVATE KEY-----\n",
        "MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQDl9h1tPpv62Ewr\n",
        "P/Ok3moNrYcC3oStE0y9Ao4dXahKYzSqIWHNaJ3evtf+hhnYdBnmt2cOqkxn3D0W\n",
        "99w34b3ZIZzL+TdluxkOWPfBbj2s2kgciAGS16Bt0OasUjJb6+wkBQoKd+WhT8iP\n",
        "jF5jgN+ujvXQ+IolJsNpT4ycjkkLjlWQpLDK25DtCJS+WKuBX7KIVvgtHaEvy9ZK\n",
        "9cumMan6OHRnlsf3jSCQ10QoX7Tk9Qodt6ZcYq7iVAqmgFVuqq5IofNfVzCbFXqe\n",
        "oa9n3pm86ETHcH4pXQOSx1J9im9Z0a2jecist/sbn9rkM+5bkRHHWtDHdU3sFDrD\n",
        "ku+WQV99AgMBAAECggEAKuOXI2vc7ZDvy9U2nNY6k2h82MUlm54Q3uOeG83++Di+\n",
        "dsiZFBVh9ExFvpvGMD+fIQ+tseeDLo+9+Q2rTeTVYqzJMKW/dkLJ7oobU0E7UYS4\n",
        "lFGtcXSz4CdpDlSaPdinhyRFdiRceJSHxHYamJZNoaHaKOph4YH0Sizi/cPvza6Y\n",
        "snMAynh/qayN9DW94QBXTgSdaCBlSPUWkgRBS7TB8gOaihrredwBtUL5T66dDKd0\n",
        "BYc/yvG+JPRXxS6+nf7GzB7wbO6wxqqWWofHCsPIhE61r6SZGlPRaCV2Z4gfihOa\n",
        "RkSXYe/0uwPA7eS60LLuKqcDTWxhFyOtL3aRyhFDIQKBgQD3r2YiGfITdygUq74j\n",
        "pgx6CAgkMNmmc7foaoUMerNi+UY+aJVXgtnAnbeoYBiWjnrmnAo3bt0ycKpHWBAb\n",
        "xezOkDMeuIeJBvJLKGMFHwexh9d6q3YlL6LeFWAU6jqKC0d/KUFw9Zu84tIu1yPP\n",
        "NxA8oASEeYHKhJhHk34mRVkzeQKBgQDtrmX4HSUtk3IMfKvJas8D1xTDFsOVVjgB\n",
        "P3JU++g4U589WoWVouSvswUqQF97bccCqqN1ChlULL8sb1SwGoXaTE3PsMczBGPs\n",
        "Wr4OaDJNIpaPSnJs79J9NF+1xhyKDJAUl9vcV+QpYhL6M3CTT4ceYg+HYPRHCExL\n",
        "lwwxqtGnJQKBgGc2kkr3oOb3qp4ii1NzqJNZsXrTWH+CjUquyM7QetxtoBX1ovYa\n",
        "sv8POi9SDC0BJrneoGtFKawRQwQD8IKCUhIqCQNuRhyV5nXnWIwTyDL0tpiJxuvs\n",
        "E+ST57wPd2F7HcohuQGHd+SYaJnYUTXSblO1IuzJbYqlAoghMFpmX6uRAoGAawmh\n",
        "Lk2h0paWQ/1ZN8n8FJSl6v2zcutzKUyzZoZTnBo8mlrL0gmbx2xtpQt8dI+Jki/a\n",
        "kPjNU1Ubex1gHDa1lQvL9v57lwQhI+3xrXdLP+WVvE4gneKa/nu5ppjjfWAO+tcw\n",
        "0lhajjB2r2q/BfhIt2wd5i/Jkruv8FLD0RJGfGkCgYAwBUWW/H0fN0YvFbVCky+g\n",
        "snJ4mU7bC5ZPwmPS75go8piUM+Q0tYvSN9SXU5SQHLRNcmVleSbF++tNbGlQ4t9B\n",
        "w4B0XZuX/QC1ghhhEPuQMABpW5QrRsZzpTd+S0oQLyyT8kazde1QdupnzSS5/Txx\n",
        "5fu0CQjvrvCXogbBrw1N+w==\n",
        "-----END PRIVATE KEY-----\n",
    );

    #[test]
    fn with_config_builds_successfully_with_a_client_identity() {
        let result = ReqwestExecutor::with_config(ClientConfig {
            client_identity_pem: Some(TEST_IDENTITY_PEM.as_bytes().to_vec()),
            ..Default::default()
        });

        assert!(result.is_ok());
    }

    #[test]
    fn with_config_errors_on_an_invalid_client_identity_pem() {
        let result = ReqwestExecutor::with_config(ClientConfig {
            client_identity_pem: Some(b"not a pem".to_vec()),
            ..Default::default()
        });

        assert!(result.is_err());
    }
}
