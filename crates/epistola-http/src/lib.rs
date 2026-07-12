//! reqwest-based `HttpExecutor` for epistola — a thin adapter between
//! `epistola_core::Request` and `reqwest::Request`.

use std::time::Instant;

use async_trait::async_trait;
use epistola_core::{ExecutorError, Header, HttpExecutor, Request, Response};

#[derive(Debug, Clone, Default)]
pub struct ReqwestExecutor {
    client: reqwest::Client,
}

impl ReqwestExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
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
            .map_err(|err| ExecutorError::Transport(Box::new(err)))?;
        let duration = start.elapsed();

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
            .map_err(|err| ExecutorError::Transport(Box::new(err)))?
            .to_vec();

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
}
