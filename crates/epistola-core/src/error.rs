use std::time::Duration;

/// Error surface for `HttpExecutor` implementations.
///
/// `Transport` boxes the backend's native error (e.g. `reqwest::Error`) so
/// swapping the HTTP backend never leaks its error type into `epistola-core`.
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("request timed out after {0:?}")]
    Timeout(Duration),

    #[error("transport error: {0}")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Error surface for `interpolate` / `interpolate_request`.
#[derive(Debug, thiserror::Error)]
pub enum InterpolationError {
    #[error("unknown variable '{0}': no value provided by any resolver layer")]
    UnknownVariable(String),

    #[error("unsupported expression '{0}' inside {{{{ }}}} — v1 only supports plain variable substitution")]
    UnsupportedExpression(String),

    #[error("template rendering failed: {0}")]
    Render(#[from] minijinja::Error),
}
