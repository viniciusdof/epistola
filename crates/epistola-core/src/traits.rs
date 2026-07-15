use crate::error::ExecutorError;
use crate::request::Request;
use crate::response::Response;

/// Executes a `Request` and produces a `Response`. The seam every HTTP
/// backend implements — `epistola-http` provides the default one.
pub trait HttpExecutor: Send + Sync {
    fn execute(
        &self,
        request: &Request,
    ) -> impl std::future::Future<Output = Result<Response, ExecutorError>> + Send;
}

/// Resolves `{{variable}}`-style placeholders against an environment.
pub trait VariableResolver: Send + Sync {
    fn resolve(&self, name: &str) -> Option<String>;
}
