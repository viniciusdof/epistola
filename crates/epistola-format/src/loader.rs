use std::path::{Path, PathBuf};

use epistola_core::LayeredVariableResolver;

use crate::collection::CollectionManifest;
use crate::discovery::find_collection_root;
use crate::environment::load_environment;
use crate::error::FormatError;
use crate::global_config::load_global_config;

/// A discovered, loaded collection (root directory + parsed manifest).
#[derive(Debug, Clone)]
pub struct LoadedCollection {
    pub root: PathBuf,
    pub manifest: CollectionManifest,
}

impl LoadedCollection {
    /// Walks up from `start` to find `epistola.toml`, then loads it.
    pub fn discover_from(start: &Path) -> Result<Self, FormatError> {
        let root = find_collection_root(start)?;
        let manifest = CollectionManifest::load(&root.join("epistola.toml"))?;
        Ok(Self { root, manifest })
    }

    /// Resolver seeded with global config, collection variables, and (if
    /// given) the named environment — low to high precedence. Callers layer
    /// request-level/CLI overrides on top via `.layer(...)`.
    pub fn resolver_for_environment(
        &self,
        environment: Option<&str>,
    ) -> Result<LayeredVariableResolver, FormatError> {
        let mut resolver = LayeredVariableResolver::new()
            .layer(load_global_config()?)
            .layer(self.manifest.variables.clone());

        if let Some(name) = environment {
            resolver = resolver.layer(load_environment(&self.root, name)?);
        }

        Ok(resolver)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_core::VariableResolver;
    use tempfile::tempdir;

    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn discover_from_locates_and_loads_the_manifest() {
        let dir = tempdir().unwrap();
        write(dir.path(), "epistola.toml", "name = \"My collection\"\n");
        let nested = dir.path().join("users");
        std::fs::create_dir_all(&nested).unwrap();

        let loaded = LoadedCollection::discover_from(&nested).unwrap();
        assert_eq!(loaded.manifest.name, "My collection");
    }

    #[test]
    fn resolver_layers_collection_over_environment_with_environment_winning() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "epistola.toml",
            "name = \"n\"\n\n[variables]\nk = \"collection\"\n",
        );
        write(
            dir.path(),
            "environments/dev.toml",
            "[variables]\nk = \"env\"\n",
        );

        let loaded = LoadedCollection::discover_from(dir.path()).unwrap();
        let resolver = loaded.resolver_for_environment(Some("dev")).unwrap();
        assert_eq!(resolver.resolve("k").as_deref(), Some("env"));
    }

    #[test]
    fn resolver_without_a_named_environment_skips_that_layer() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "epistola.toml",
            "name = \"n\"\n\n[variables]\nk = \"collection\"\n",
        );

        let loaded = LoadedCollection::discover_from(dir.path()).unwrap();
        let resolver = loaded.resolver_for_environment(None).unwrap();
        assert_eq!(resolver.resolve("k").as_deref(), Some("collection"));
    }

    #[test]
    fn resolver_propagates_environment_not_found() {
        let dir = tempdir().unwrap();
        write(dir.path(), "epistola.toml", "name = \"n\"\n");

        let loaded = LoadedCollection::discover_from(dir.path()).unwrap();
        let result = loaded.resolver_for_environment(Some("prod"));
        assert!(matches!(
            result,
            Err(FormatError::EnvironmentNotFound { .. })
        ));
    }
}
