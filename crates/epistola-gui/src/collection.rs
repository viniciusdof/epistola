use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use epistola_core::Method;
use epistola_engine::discovery::discover_collection;
use epistola_engine::environments::list_environment_names;
use epistola_engine::requests::list_requests;
use epistola_engine::EngineError;
use gpui::{Context, WeakEntity};

use crate::root::EpistolaGui;

#[derive(Clone, Debug)]
pub struct RequestEntry {
    pub abs_path: PathBuf,
    pub rel_path: PathBuf,
    pub file_name: String,
    pub display_name: String,
    pub method: Method,
}

#[derive(Clone, Debug, Default)]
pub struct FolderEntry {
    pub name: String,
    pub rel_path: PathBuf,
    pub has_folder_toml: bool,
    pub folders: Vec<FolderEntry>,
    pub requests: Vec<RequestEntry>,
}

#[derive(Clone, Debug)]
pub struct CollectionTree {
    pub root: PathBuf,
    pub name: String,
    pub folders: Vec<FolderEntry>,
    pub requests: Vec<RequestEntry>,
    pub environments: Vec<String>,
    pub default_environment: Option<String>,
    pub(crate) index: BTreeMap<PathBuf, RequestEntry>,
}

impl CollectionTree {
    pub fn all_requests(&self) -> Vec<&RequestEntry> {
        self.index.values().collect()
    }

    pub fn find_request(&self, abs_path: &Path) -> Option<&RequestEntry> {
        self.index.get(abs_path)
    }
}

/// Walks/creates `FolderEntry` nodes down to `dir`, returning the leaf node.
fn ensure_folder<'a>(root: &'a mut FolderEntry, dir: &Path) -> &'a mut FolderEntry {
    let mut current = root;
    let mut rel_accum = PathBuf::new();
    for component in dir.components() {
        let name = component.as_os_str().to_string_lossy().into_owned();
        rel_accum.push(&name);
        let idx = match current.folders.iter().position(|f| f.name == name) {
            Some(idx) => idx,
            None => {
                current.folders.push(FolderEntry {
                    name,
                    rel_path: rel_accum.clone(),
                    ..Default::default()
                });
                current.folders.len() - 1
            }
        };
        current = &mut current.folders[idx];
    }
    current
}

fn insert_request(root: &mut FolderEntry, dir: &Path, entry: RequestEntry) {
    ensure_folder(root, dir).requests.push(entry);
}

fn mark_folder_toml_presence(collection_root: &Path, folder: &mut FolderEntry) {
    folder.has_folder_toml = collection_root
        .join(&folder.rel_path)
        .join("folder.toml")
        .is_file();
    for child in &mut folder.folders {
        mark_folder_toml_presence(collection_root, child);
    }
}

/// Files that fail to parse are skipped rather than failing the whole tree
/// — one bad `.req.toml` shouldn't lock a user out of every other request.
/// (`list_requests` also reports them typed as `invalid`; surfacing those in
/// the sidebar instead of dropping them is a follow-up.)
pub fn load(cwd: &Path) -> Result<CollectionTree, EngineError> {
    let collection = discover_collection(cwd)?;
    let listing = list_requests(&collection)?;

    let mut root_folder = FolderEntry::default();
    let mut index = BTreeMap::new();
    for summary in listing.requests {
        let file_name = summary
            .rel_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = summary
            .rel_path
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf();
        let entry = RequestEntry {
            abs_path: summary.abs_path,
            rel_path: summary.rel_path,
            file_name,
            display_name: summary.name,
            method: summary.method,
        };
        index.insert(entry.abs_path.clone(), entry.clone());
        insert_request(&mut root_folder, &dir, entry);
    }
    for abs_dir in epistola_engine::folder::find_folder_manifest_dirs(&collection.root)? {
        if let Ok(rel_dir) = abs_dir.strip_prefix(&collection.root) {
            ensure_folder(&mut root_folder, rel_dir);
        }
    }
    mark_folder_toml_presence(&collection.root, &mut root_folder);

    let environments = list_environment_names(&collection)
        .map(|set| set.into_iter().collect())
        .unwrap_or_default();

    Ok(CollectionTree {
        root: collection.root.clone(),
        name: collection.manifest.name.clone(),
        folders: root_folder.folders,
        requests: root_folder.requests,
        environments,
        default_environment: collection.manifest.default_environment.clone(),
        index,
    })
}

/// Loads `cwd` off the main thread and hands the result to
/// `AppState::apply_opened_collection`, which drops it if `cwd` has changed since.
pub fn spawn_load_collection(cwd: PathBuf, cx: &mut Context<EpistolaGui>) {
    let load_cwd = cwd.clone();
    cx.spawn(async move |weak: WeakEntity<EpistolaGui>, cx| {
        let result = cx
            .background_executor()
            .spawn(async_compat::Compat::new(async move { load(&load_cwd) }))
            .await
            .map_err(|err| err.to_string());
        let _ = weak.update(cx, |gui, cx| {
            gui.state.apply_opened_collection(cwd, result);
            gui.respawn_watcher(cx);
            cx.notify();
        });
    })
    .detach();
}

pub fn spawn_refresh_collection(cwd: PathBuf, cx: &mut Context<EpistolaGui>) {
    cx.spawn(async move |weak: WeakEntity<EpistolaGui>, cx| {
        let result = cx
            .background_executor()
            .spawn(async_compat::Compat::new(async move { load(&cwd) }))
            .await
            .map_err(|err| err.to_string());
        let _ = weak.update(cx, |gui, cx| {
            gui.state.apply_refreshed_collection(result);
            cx.notify();
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_format::CollectionManifest;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn load_renders_an_empty_folder_that_has_no_requests_yet() {
        let dir = tempdir().unwrap();
        CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None).unwrap();
        epistola_engine::folder::init_folder(dir.path(), "auth").unwrap();

        let tree = load(dir.path()).unwrap();

        assert_eq!(tree.folders.len(), 1);
        assert_eq!(tree.folders[0].name, "auth");
        assert!(tree.folders[0].has_folder_toml);
        assert!(tree.folders[0].requests.is_empty());
    }
}
