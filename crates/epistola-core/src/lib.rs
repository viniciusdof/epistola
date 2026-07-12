//! Core domain types and extension traits for epistola: `Request`,
//! `Response`, and the seams (`HttpExecutor`, `AuthProvider`,
//! `VariableResolver`, `ScriptHook`) other crates implement.

mod body;
mod error;
mod interpolation;
mod method;
mod request;
mod resolver;
mod response;
mod traits;

pub use body::Body;
pub use error::{AuthError, ExecutorError, InterpolationError, ScriptError};
pub use interpolation::{interpolate, interpolate_request};
pub use method::Method;
pub use request::{Header, Request};
pub use resolver::LayeredVariableResolver;
pub use response::Response;
pub use traits::{AuthProvider, HttpExecutor, NoopScriptHook, ScriptHook, VariableResolver};
