use std::path::PathBuf;

/// Error surface for all of `epistola-format`'s parsing/loading operations.
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("failed to read '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse TOML: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("failed to parse TOML in '{path}': {source}")]
    TomlAt {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("failed to serialize TOML for '{path}': {source}")]
    TomlSerialize {
        path: PathBuf,
        #[source]
        source: toml::ser::Error,
    },

    #[error(transparent)]
    Interpolation(#[from] epistola_core::InterpolationError),

    #[error("no 'epistola.toml' found in '{start}' or any parent directory")]
    CollectionRootNotFound { start: PathBuf },

    #[error("environment '{name}' not found (expected '{path}')")]
    EnvironmentNotFound { name: String, path: PathBuf },

    #[error("could not determine the user's config directory")]
    NoHomeDirectory,

    #[error("'{path}' already exists")]
    AlreadyExists { path: PathBuf },
}
