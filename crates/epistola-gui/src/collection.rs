use std::path::{Path, PathBuf};

use epistola_core::Method;
use epistola_engine::discovery::discover_collection;
use epistola_engine::environments::list_environment_names;
use epistola_engine::requests::find_request_files;
use epistola_engine::EngineError;
use epistola_format::RequestFile;

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
}

impl CollectionTree {
    pub fn all_requests(&self) -> Vec<&RequestEntry> {
        let mut out: Vec<&RequestEntry> = self.requests.iter().collect();
        for folder in &self.folders {
            folder.collect_requests(&mut out);
        }
        out
    }

    pub fn find_request(&self, abs_path: &Path) -> Option<&RequestEntry> {
        self.all_requests()
            .into_iter()
            .find(|r| r.abs_path == abs_path)
    }
}

impl FolderEntry {
    fn collect_requests<'a>(&'a self, out: &mut Vec<&'a RequestEntry>) {
        out.extend(self.requests.iter());
        for child in &self.folders {
            child.collect_requests(out);
        }
    }
}

fn insert_request(root: &mut FolderEntry, dir: &Path, entry: RequestEntry) {
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
    current.requests.push(entry);
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
pub fn load(cwd: &Path) -> Result<CollectionTree, EngineError> {
    let collection = discover_collection(cwd)?;
    let mut paths = find_request_files(&collection.root)?;
    paths.sort();

    let mut root_folder = FolderEntry::default();
    for path in paths {
        let Ok(file) = RequestFile::load(&path) else {
            continue;
        };
        let rel_path = path
            .strip_prefix(&collection.root)
            .unwrap_or(&path)
            .to_path_buf();
        let file_name = rel_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = rel_path.parent().unwrap_or(Path::new(""));
        let entry = RequestEntry {
            abs_path: path,
            rel_path: rel_path.clone(),
            file_name,
            display_name: file.request.name.clone(),
            method: Method::from(file.request.method.as_str()),
        };
        insert_request(&mut root_folder, dir, entry);
    }
    mark_folder_toml_presence(&collection.root, &mut root_folder);

    let environments = list_environment_names(cwd)
        .map(|set| set.into_iter().collect())
        .unwrap_or_default();

    Ok(CollectionTree {
        root: collection.root.clone(),
        name: collection.manifest.name.clone(),
        folders: root_folder.folders,
        requests: root_folder.requests,
        environments,
        default_environment: collection.manifest.default_environment.clone(),
    })
}
