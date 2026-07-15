use std::collections::HashMap;

use minijinja::{Environment, ErrorKind, UndefinedBehavior};

use crate::body::Body;
use crate::error::InterpolationError;
use crate::request::{Header, Request};
use crate::traits::VariableResolver;

pub fn interpolate(
    input: &str,
    resolver: &dyn VariableResolver,
) -> Result<String, InterpolationError> {
    let mut env = Environment::empty();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    let template = env
        .template_from_str(input)
        .map_err(InterpolationError::Render)?;

    let mut context = HashMap::new();
    let mut unresolved = Vec::new();
    for name in template.undeclared_variables(false) {
        match resolver.resolve(&name) {
            Some(value) => {
                context.insert(name, value);
            }
            None => unresolved.push(name),
        }
    }
    unresolved.sort();

    template.render(context).map_err(|err| match err.kind() {
        ErrorKind::UndefinedError => match unresolved.into_iter().next() {
            Some(name) => InterpolationError::UnknownVariable(name),
            None => InterpolationError::Render(err),
        },
        ErrorKind::UnknownFilter | ErrorKind::UnknownTest | ErrorKind::UnknownFunction => {
            InterpolationError::UnsupportedExpression(err.to_string())
        }
        _ => InterpolationError::Render(err),
    })
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
    fn evaluates_a_pure_expression_instead_of_rejecting_it() {
        let r = resolver(&[]);
        assert_eq!(interpolate("{{ 1 + 2 }}", &r).unwrap(), "3");
    }

    #[test]
    fn rejects_an_unknown_function_call_as_unsupported() {
        let r = resolver(&[]);
        let err = interpolate("{{ range(3) }}", &r).unwrap_err();
        assert!(matches!(err, InterpolationError::UnsupportedExpression(_)));
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
