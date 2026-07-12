//! Parses epistola collection/request files (`.req.toml`, `epistola.toml`,
//! environments, global config) into `epistola-core` types. Parsing leaves
//! `{{variable}}` placeholders unresolved — call
//! [`UnresolvedRequest::resolve`] with a resolver to get a send-ready
//! `Request`.

mod collection;
mod discovery;
mod environment;
mod error;
mod global_config;
mod loader;
mod request_file;
mod toml_file;
mod variables_file;

pub use collection::CollectionManifest;
pub use discovery::find_collection_root;
pub use environment::{create_environment, load_environment, set_environment_variable};
pub use error::FormatError;
pub use global_config::load_global_config;
pub use loader::LoadedCollection;
pub use request_file::{
    AuthSpec, BodySpec, FormField, HeaderEntry, QueryEntry, RequestFile, RequestSpec,
    UnresolvedRequest,
};
