//! Plain-text formatting for a resolved request — shared by the CLI and
//! GUI so a saved request renders identically in both places.

use epistola_core::{Body, Request};

pub fn format_request_text(request: &Request) -> String {
    let mut out = format!("{} {}\n", request.method.as_str(), request.url);
    if !request.query.is_empty() {
        out.push_str("\nQuery:\n");
        for (name, value) in &request.query {
            out.push_str(&format!("  {name} = {value}\n"));
        }
    }
    if !request.headers.is_empty() {
        out.push_str("\nHeaders:\n");
        for header in &request.headers {
            out.push_str(&format!("  {}: {}\n", header.name, header.value));
        }
    }
    if let Body::Bytes(bytes) = &request.body {
        out.push_str("\nBody:\n");
        out.push_str(&String::from_utf8_lossy(bytes));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_core::Method;

    use super::*;

    #[test]
    fn includes_method_and_url() {
        let request = Request::get("https://x.test");
        assert!(format_request_text(&request).starts_with("GET https://x.test"));
    }

    #[test]
    fn lists_query_params_under_a_query_heading() {
        let request = Request::new(Method::Get, "https://x.test").query("page", "1");
        let out = format_request_text(&request);
        assert!(out.contains("Query:\n  page = 1\n"));
    }

    #[test]
    fn lists_headers_under_a_headers_heading() {
        let request =
            Request::new(Method::Get, "https://x.test").header("Authorization", "Bearer abc");
        let out = format_request_text(&request);
        assert!(out.contains("Headers:\n  Authorization: Bearer abc\n"));
    }

    #[test]
    fn includes_a_bytes_body_under_a_body_heading() {
        let request = Request::new(Method::Post, "https://x.test").body(Body::text("payload"));
        let out = format_request_text(&request);
        assert!(out.contains("Body:\npayload"));
    }

    #[test]
    fn omits_empty_sections() {
        let request = Request::get("https://x.test");
        let out = format_request_text(&request);
        assert!(!out.contains("Query:"));
        assert!(!out.contains("Headers:"));
        assert!(!out.contains("Body:"));
    }
}
