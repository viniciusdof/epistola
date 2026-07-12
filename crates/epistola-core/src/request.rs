use crate::body::Body;
use crate::method::Method;

/// A single HTTP header field. A pair, not a map — headers may repeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

impl Header {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// A backend-agnostic HTTP request. No execution state (timeouts, TLS) —
/// that belongs to the executor.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<Header>,
    pub query: Vec<(String, String)>,
    pub body: Body,
}

impl Request {
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            query: Vec::new(),
            body: Body::Empty,
        }
    }

    pub fn get(url: impl Into<String>) -> Self {
        Self::new(Method::Get, url)
    }

    pub fn post(url: impl Into<String>) -> Self {
        Self::new(Method::Post, url)
    }

    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push(Header::new(name, value));
        self
    }

    #[must_use]
    pub fn query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn body(mut self, body: Body) -> Self {
        self.body = body;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_and_post_set_the_expected_method() {
        assert_eq!(Request::get("https://x.test").method, Method::Get);
        assert_eq!(Request::post("https://x.test").method, Method::Post);
    }

    #[test]
    fn builder_accumulates_headers_and_query_in_order() {
        let request = Request::get("https://x.test")
            .header("A", "1")
            .header("B", "2")
            .query("x", "1")
            .query("y", "2")
            .body(Body::text("payload"));

        assert_eq!(
            request.headers,
            vec![Header::new("A", "1"), Header::new("B", "2")]
        );
        assert_eq!(
            request.query,
            vec![
                ("x".to_string(), "1".to_string()),
                ("y".to_string(), "2".to_string())
            ]
        );
        assert_eq!(request.body, Body::text("payload"));
    }

    #[test]
    fn new_request_has_no_headers_query_or_body() {
        let request = Request::new(Method::Get, "https://x.test");
        assert!(request.headers.is_empty());
        assert!(request.query.is_empty());
        assert_eq!(request.body, Body::Empty);
    }
}
