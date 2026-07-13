use std::path::Path;

use epistola_core::{Header, Request, Response};

/// Status line, headers, blank line, then body (pretty JSON if applicable).
pub fn format_response(response: &Response) -> String {
    let mut out = format_response_head(response);
    out.push_str(&format_body(response));
    out.push('\n');
    out
}

/// Status line and headers only (no body) — used when the body is instead
/// written to a file via `--output`.
pub fn format_response_head(response: &Response) -> String {
    let mut out = format!("HTTP {} ({:.0?})\n", response.status, response.duration);

    for header in &response.headers {
        out.push_str(&format!("{}: {}\n", header.name, header.value));
    }
    out.push('\n');
    out
}

/// Writes the response body's raw bytes to `path`, unchanged.
pub fn write_response_body(response: &Response, path: &Path) -> std::io::Result<()> {
    std::fs::write(path, &response.body)
}

fn format_body(response: &Response) -> String {
    match response.body_as_str() {
        Ok(text) => format_text_body(text, &response.headers),
        Err(_) => format!("<{} bytes of binary data>", response.body.len()),
    }
}

fn format_text_body(text: &str, headers: &[Header]) -> String {
    let is_json = headers
        .iter()
        .any(|h| h.name.eq_ignore_ascii_case("content-type") && h.value.contains("json"));

    if is_json {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
            if let Ok(pretty) = serde_json::to_string_pretty(&value) {
                return pretty;
            }
        }
    }

    text.to_string()
}

pub fn response_to_json(response: &Response) -> serde_json::Value {
    serde_json::json!({
        "status": response.status,
        "duration_ms": response.duration.as_millis(),
        "headers": response.headers.iter().map(header_to_json).collect::<Vec<_>>(),
        "body": response.body_as_str().ok(),
    })
}

/// Like `format_response`, but for a request that hasn't been sent yet.
pub fn format_request(request: &Request) -> String {
    let mut out = format!("{} {}\n", request.method, request.url);

    for (key, value) in &request.query {
        out.push_str(&format!("  ?{key}={value}\n"));
    }
    for header in &request.headers {
        out.push_str(&format!("{}: {}\n", header.name, header.value));
    }
    out.push('\n');

    match std::str::from_utf8(request.body.as_bytes()) {
        Ok(text) if !text.is_empty() => out.push_str(text),
        Ok(_) => {}
        Err(_) => out.push_str(&format!("<{} bytes of binary data>", request.body.len())),
    }
    out.push('\n');
    out
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

    use epistola_core::{Body, Method};

    use super::*;

    fn response(headers: Vec<Header>, body: &[u8]) -> Response {
        Response {
            status: 200,
            headers,
            body: body.to_vec(),
            duration: Duration::ZERO,
        }
    }

    #[test]
    fn pretty_prints_a_json_body() {
        let headers = vec![Header::new("content-type", "application/json")];
        let out = format_response(&response(headers, br#"{"a":1}"#));
        assert!(out.contains("\"a\": 1"), "expected pretty JSON, got: {out}");
    }

    #[test]
    fn leaves_non_json_content_type_untouched() {
        let headers = vec![Header::new("content-type", "text/plain")];
        let out = format_response(&response(headers, br#"{"a":1}"#));
        assert!(out.contains(r#"{"a":1}"#), "expected raw body, got: {out}");
    }

    #[test]
    fn falls_back_to_raw_text_on_invalid_json() {
        let headers = vec![Header::new("content-type", "application/json")];
        let out = format_response(&response(headers, b"not json"));
        assert!(
            out.contains("not json"),
            "expected raw fallback, got: {out}"
        );
    }

    #[test]
    fn reports_byte_count_for_binary_bodies() {
        let out = format_response(&response(Vec::new(), &[0xff, 0xfe, 0x00]));
        assert!(out.contains("<3 bytes of binary data>"), "got: {out}");
    }

    #[test]
    fn includes_status_and_headers() {
        let headers = vec![Header::new("x-test", "epistola")];
        let out = format_response(&response(headers, b"ok"));
        assert!(out.starts_with("HTTP 200"));
        assert!(out.contains("x-test: epistola"));
    }

    #[test]
    fn format_response_head_omits_the_body() {
        let out = format_response_head(&response(Vec::new(), b"secret-body"));
        assert!(out.starts_with("HTTP 200"));
        assert!(!out.contains("secret-body"));
    }

    #[test]
    fn write_response_body_writes_the_raw_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("body.bin");
        write_response_body(&response(Vec::new(), &[0xff, 0x00, 0x42]), &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), vec![0xff, 0x00, 0x42]);
    }

    #[test]
    fn response_to_json_includes_status_and_body() {
        let headers = vec![Header::new("x-test", "epistola")];
        let json = response_to_json(&response(headers, b"ok"));
        assert_eq!(json["status"], 200);
        assert_eq!(json["body"], "ok");
        assert_eq!(json["headers"][0]["name"], "x-test");
    }

    #[test]
    fn format_request_includes_method_url_headers_and_body() {
        let request = Request::new(Method::Post, "https://x.test/users")
            .query("page", "1")
            .header("Authorization", "Bearer abc")
            .body(Body::text("payload"));
        let out = format_request(&request);
        assert!(out.starts_with("POST https://x.test/users"));
        assert!(out.contains("?page=1"));
        assert!(out.contains("Authorization: Bearer abc"));
        assert!(out.contains("payload"));
    }

    #[test]
    fn request_to_json_includes_method_and_url() {
        let request = Request::get("https://x.test");
        let json = request_to_json(&request);
        assert_eq!(json["method"], "GET");
        assert_eq!(json["url"], "https://x.test");
    }
}
