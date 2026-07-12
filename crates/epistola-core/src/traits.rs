use async_trait::async_trait;

use crate::error::{AuthError, ExecutorError, ScriptError};
use crate::request::Request;
use crate::response::Response;

/// Executes a `Request` and produces a `Response`. The seam every HTTP
/// backend implements — `epistola-http` provides the default one.
#[async_trait]
pub trait HttpExecutor: Send + Sync {
    async fn execute(&self, request: &Request) -> Result<Response, ExecutorError>;
}

/// Resolves `{{variable}}`-style placeholders against an environment.
pub trait VariableResolver: Send + Sync {
    fn resolve(&self, name: &str) -> Option<String>;
}

/// Applies authentication to a request in place.
pub trait AuthProvider: Send + Sync {
    fn apply(&self, request: &mut Request) -> Result<(), AuthError>;
}

/// Pre-request / post-response hook — the seam future scripting plugs into.
#[async_trait]
pub trait ScriptHook: Send + Sync {
    async fn pre_request(&self, _request: &mut Request) -> Result<(), ScriptError> {
        Ok(())
    }

    async fn post_response(&self, _response: &mut Response) -> Result<(), ScriptError> {
        Ok(())
    }
}

/// A `ScriptHook` that does nothing — the default until a real one exists.
pub struct NoopScriptHook;

#[async_trait]
impl ScriptHook for NoopScriptHook {}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use crate::{Method, Request, Response};

    use super::*;

    #[tokio::test]
    async fn noop_hook_leaves_the_request_and_response_untouched() {
        let mut request = Request::new(Method::Get, "https://x.test");
        NoopScriptHook.pre_request(&mut request).await.unwrap();
        assert_eq!(request.url, "https://x.test");

        let mut response = Response {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            duration: std::time::Duration::ZERO,
        };
        NoopScriptHook.post_response(&mut response).await.unwrap();
        assert_eq!(response.status, 200);
    }
}
