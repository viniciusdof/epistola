use std::collections::HashMap;

use minijinja::Environment;

use crate::body::Body;
use crate::error::InterpolationError;
use crate::request::{Header, Request};
use crate::traits::VariableResolver;

fn is_simple_variable_name(candidate: &str) -> bool {
    let mut chars = candidate.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Returns the raw (trimmed, unresolved) content of every `{{ ... }}`
/// placeholder in `input`, in order of appearance.
fn scan_placeholders(input: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        let after_open = &rest[start + 2..];
        match after_open.find("}}") {
            Some(end) => {
                found.push(after_open[..end].trim());
                rest = &after_open[end + 2..];
            }
            None => break, // unterminated `{{` — minijinja itself will report this clearly
        }
    }
    found
}

/// Substitutes every `{{ name }}` placeholder using `resolver`, then
/// renders through minijinja. Only bare variable names are supported —
/// `{{ x | upper }}` errors as `UnsupportedExpression` rather than being
/// silently accepted. Fails fast with `UnknownVariable` on the first
/// unresolved placeholder.
pub fn interpolate(
    input: &str,
    resolver: &dyn VariableResolver,
) -> Result<String, InterpolationError> {
    let mut context = HashMap::new();
    for placeholder in scan_placeholders(input) {
        if !is_simple_variable_name(placeholder) {
            return Err(InterpolationError::UnsupportedExpression(
                placeholder.to_string(),
            ));
        }
        let value = resolver
            .resolve(placeholder)
            .ok_or_else(|| InterpolationError::UnknownVariable(placeholder.to_string()))?;
        context.insert(placeholder.to_string(), value);
    }

    let env = Environment::new();
    env.render_str(input, context)
        .map_err(InterpolationError::Render)
}

/// Interpolates every string on a `Request` (url, header values, query
/// values, and — if the body is valid UTF-8 — the body). Header/query
/// *names* are not interpolated. A non-UTF-8 body passes through untouched.
pub fn interpolate_request(
    request: &Request,
    resolver: &dyn VariableResolver,
) -> Result<Request, InterpolationError> {
    let url = interpolate(&request.url, resolver)?;

    let headers = request
        .headers
        .iter()
        .map(|h| {
            Ok(Header::new(
                h.name.clone(),
                interpolate(&h.value, resolver)?,
            ))
        })
        .collect::<Result<Vec<_>, InterpolationError>>()?;

    let query = request
        .query
        .iter()
        .map(|(k, v)| Ok((k.clone(), interpolate(v, resolver)?)))
        .collect::<Result<Vec<_>, InterpolationError>>()?;

    let body = match &request.body {
        Body::Empty => Body::Empty,
        Body::Bytes(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => Body::text(interpolate(text, resolver)?),
            Err(_) => Body::Bytes(bytes.clone()),
        },
    };

    Ok(Request {
        method: request.method.clone(),
        url,
        headers,
        query,
        body,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::collections::BTreeMap;

    use crate::{Body, LayeredVariableResolver, Method};

    use super::*;

    fn resolver(pairs: &[(&str, &str)]) -> LayeredVariableResolver {
        LayeredVariableResolver::new().layer(BTreeMap::from_iter(
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())),
        ))
    }

    #[test]
    fn replaces_a_single_placeholder() {
        let r = resolver(&[("name", "world")]);
        assert_eq!(interpolate("hello {{name}}", &r).unwrap(), "hello world");
    }

    #[test]
    fn replaces_multiple_placeholders_in_one_string() {
        let r = resolver(&[("a", "1"), ("b", "2")]);
        assert_eq!(interpolate("{{a}}-{{b}}", &r).unwrap(), "1-2");
    }

    #[test]
    fn leaves_plain_text_untouched() {
        let r = resolver(&[]);
        assert_eq!(
            interpolate("no placeholders here", &r).unwrap(),
            "no placeholders here"
        );
    }

    #[test]
    fn trims_whitespace_inside_braces() {
        let r = resolver(&[("x", "1")]);
        assert_eq!(interpolate("{{ x }}", &r).unwrap(), "1");
    }

    #[test]
    fn errors_on_unknown_variable_with_the_name_in_the_message() {
        let r = resolver(&[]);
        let err = interpolate("{{missing}}", &r).unwrap_err();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn rejects_a_filter_expression_as_unsupported_rather_than_unknown() {
        let r = resolver(&[("x", "1")]);
        let err = interpolate("{{ x | upper }}", &r).unwrap_err();
        assert!(matches!(err, InterpolationError::UnsupportedExpression(_)));
    }

    #[test]
    fn unterminated_double_brace_is_a_render_error() {
        let r = resolver(&[]);
        let err = interpolate("broken {{ open", &r).unwrap_err();
        assert!(matches!(err, InterpolationError::Render(_)));
    }

    #[test]
    fn interpolate_request_resolves_url_headers_query_and_text_body() {
        let r = resolver(&[("host", "x.test"), ("token", "abc")]);
        let request = Request::new(Method::Get, "https://{{host}}/users")
            .header("Authorization", "Bearer {{token}}")
            .query("q", "{{token}}")
            .body(Body::text("payload {{token}}"));

        let resolved = interpolate_request(&request, &r).unwrap();

        assert_eq!(resolved.url, "https://x.test/users");
        assert_eq!(resolved.headers[0].value, "Bearer abc");
        assert_eq!(resolved.query[0].1, "abc");
        assert_eq!(resolved.body, Body::text("payload abc"));
    }

    #[test]
    fn interpolate_request_fails_fast_on_the_first_unresolved_variable() {
        let r = resolver(&[]);
        let request = Request::get("https://{{missing}}/x");
        assert!(matches!(
            interpolate_request(&request, &r),
            Err(InterpolationError::UnknownVariable(_))
        ));
    }

    #[test]
    fn interpolate_request_leaves_a_non_utf8_body_untouched() {
        let r = resolver(&[]);
        let request = Request::post("https://x.test").body(Body::Bytes(vec![0xff, 0xfe]));
        let resolved = interpolate_request(&request, &r).unwrap();
        assert_eq!(resolved.body, Body::Bytes(vec![0xff, 0xfe]));
    }
}
