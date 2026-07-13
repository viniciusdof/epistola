use std::path::PathBuf;

use epistola_core::ExecutorError;
use epistola_format::FormatError;

/// Error surface for every public function in this crate. `epistola-cli`
/// and a future GUI both match on this instead of downcasting `anyhow`.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("{message}")]
    NotInCollection {
        message: &'static str,
        #[source]
        source: Box<FormatError>,
    },

    #[error(transparent)]
    Format(Box<FormatError>),

    #[error(transparent)]
    Executor(#[from] ExecutorError),

    #[error("invalid client configuration: {0}")]
    ClientBuild(#[from] reqwest::Error),

    #[error("failed to read '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to encode JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid --var '{0}', expected KEY=VALUE")]
    InvalidVarOverride(String),

    #[error("'{path}' already exists")]
    AlreadyExists { path: PathBuf },

    #[error("HTTP status {0} indicates failure (--check-status)")]
    HttpStatusFailure(u16),

    #[error("{0} request file(s) failed lint")]
    LintFailed(usize),
}

impl From<FormatError> for EngineError {
    fn from(err: FormatError) -> Self {
        EngineError::Format(Box::new(err))
    }
}

/// The exact message used everywhere a saved-request or environment
/// operation requires a collection to exist.
pub(crate) const NOT_IN_COLLECTION: &str =
    "not inside a collection (no epistola.toml found in this or any parent directory)";
