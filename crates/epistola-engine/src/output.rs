//! Pure JSON-shape converters shared by `history::append_entry` and any
//! caller's `--json`-style output — not to be confused with terminal-text
//! formatting, which stays in `epistola-cli::output`.

use epistola_core::{Header, Request, Response};

pub fn response_to_json(response: &Response) -> serde_json::Value {
    serde_json::json!({
        "status": response.status,
        "duration_ms": response.duration.as_millis(),
        "headers": response.headers.iter().map(header_to_json).collect::<Vec<_>>(),
        "body": response.body_as_str().ok(),
    })
}

pub fn request_to_json(request: &Request) -> serde_json::Value {
    serde_json::json!({
        "method": request.method.as_str(),
        "url": request.url,
        "query": request.query.iter().map(|(k, v)| serde_json::json!({"name": k, "value": v})).collect::<Vec<_>>(),
        "headers": request.headers.iter().map(header_to_json).collect::<Vec<_>>(),
        "body": std::str::from_utf8(request.body.as_bytes()).ok(),
    })
}

fn header_to_json(header: &Header) -> serde_json::Value {
    serde_json::json!({"name": header.name, "value": header.value})
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::time::Duration;

    use epistola_core::Method;

    use super::*;

    #[test]
    fn response_to_json_includes_status_and_body() {
        let response = Response {
            status: 200,
            headers: vec![Header::new("x-test", "epistola")],
            body: b"ok".to_vec(),
            duration: Duration::ZERO,
        };
        let json = response_to_json(&response);
        assert_eq!(json["status"], 200);
        assert_eq!(json["body"], "ok");
        assert_eq!(json["headers"][0]["name"], "x-test");
    }

    #[test]
    fn request_to_json_includes_method_and_url() {
        let request = Request::get("https://x.test");
        let json = request_to_json(&request);
        assert_eq!(json["method"], "GET");
        assert_eq!(json["url"], "https://x.test");
    }

    #[test]
    fn request_to_json_uses_method_as_str_not_debug() {
        let request = Request::new(Method::Post, "https://x.test");
        assert_eq!(request_to_json(&request)["method"], "POST");
    }
}
