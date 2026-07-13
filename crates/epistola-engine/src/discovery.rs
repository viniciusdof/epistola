use std::path::Path;

use epistola_format::LoadedCollection;

use crate::error::{EngineError, NOT_IN_COLLECTION};

/// Walks up from `start` for `epistola.toml`, mapping a miss to the
/// standard "not inside a collection" message every collection-scoped
/// operation shares.
pub fn discover_collection(start: &Path) -> Result<LoadedCollection, EngineError> {
    LoadedCollection::discover_from(start).map_err(|source| EngineError::NotInCollection {
        message: NOT_IN_COLLECTION,
        source: Box::new(source),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn errors_with_the_standard_message_outside_a_collection() {
        let dir = tempdir().unwrap();
        let err = discover_collection(dir.path()).unwrap_err();
        assert_eq!(err.to_string(), NOT_IN_COLLECTION);
    }

    #[test]
    fn finds_the_collection_root() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();

        let collection = discover_collection(dir.path()).unwrap();
        assert_eq!(collection.root, dir.path());
    }
}
