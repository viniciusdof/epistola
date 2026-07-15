use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use epistola_core::Response;
use epistola_engine::history::HistoryEntry;
use epistola_format::{FolderManifest, RequestFile};
use gpui::Pixels;

use crate::buffer::EditorBuffer;
use crate::collection::{self, CollectionTree, RequestEntry};
use crate::components::response_drawer;
use crate::components::sidebar::{self, SidebarRow};
use crate::execution;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Home,
    Workspace,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ActiveFile {
    None,
    Request(PathBuf),
    Config,
    Folder(PathBuf),
    Environment(String),
}

impl ActiveFile {
    pub fn is_request(&self) -> bool {
        matches!(self, ActiveFile::Request(_))
    }

    pub fn disk_path(&self, collection: Option<&CollectionTree>) -> Option<PathBuf> {
        match self {
            ActiveFile::Request(path) => Some(path.clone()),
            ActiveFile::Folder(dir) => Some(dir.join("folder.toml")),
            ActiveFile::Environment(name) => Some(
                collection?
                    .root
                    .join("environments")
                    .join(format!("{name}.toml")),
            ),
            ActiveFile::Config | ActiveFile::None => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Overlay {
    CommandPalette,
    QuickOpen,
    EnvironmentPicker,
    History,
    Prompt(PromptKind),
}

/// A single-field prompt, rendered by `components::prompt_modal`. The
/// entered text lives on `EpistolaGui::overlay_input`
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PromptKind {
    New { dir: String },
    Rename { path: PathBuf },
    Duplicate { path: PathBuf },
}

#[derive(Clone, Debug)]
pub enum ActivityResult {
    Idle,
    Running,
    RunSuccess(Response),
    RunFailed(String),
    UnresolvedVariable { variable: String },
    Resolved(String),
    ResolvedFailed(String),
    Linted(String),
    LintFailed(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ResponseSubTab {
    #[default]
    Body,
    Headers,
    Raw,
}

pub struct UrlPreview {
    pub text: String,
    pub unresolved_variable: Option<String>,
    pub virtual_note: Option<String>,
}

pub struct AppState {
    pub cwd: PathBuf,
    pub view: View,
    pub active_file: ActiveFile,
    pub open_tabs: Vec<ActiveFile>,
    pub environment: Option<String>,
    pub collection: Result<CollectionTree, String>,
    pub overlay: Option<Overlay>,

    pub activity: HashMap<ActiveFile, ActivityResult>,
    pub response_subtab: ResponseSubTab,
    pub collection_action_error: Option<String>,
    pub recent_collections: Vec<PathBuf>,
    pub editor_buffers: HashMap<ActiveFile, EditorBuffer>,
    pub url_previews: HashMap<PathBuf, UrlPreview>,
    pub history_entries: Vec<HistoryEntry>,
    pub sidebar_rows: Vec<SidebarRow>,
    pub collapsed_folders: HashSet<PathBuf>,

    pub sidebar_width: Pixels,
    pub sidebar_collapsed: bool,
    pub drawer_height: Pixels,
    pub drawer_collapsed: bool,

    pub overlay_selected: usize,
    pub overlay_error: Option<String>,
}

impl AppState {
    pub fn new(cwd: PathBuf) -> Self {
        let mut state = Self {
            cwd: PathBuf::new(),
            view: View::Workspace,
            active_file: ActiveFile::None,
            open_tabs: Vec::new(),
            environment: None,
            collection: Err(String::new()),
            overlay: None,
            activity: HashMap::new(),
            response_subtab: ResponseSubTab::default(),
            collection_action_error: None,
            recent_collections: Vec::new(),
            editor_buffers: HashMap::new(),
            url_previews: HashMap::new(),
            history_entries: Vec::new(),
            sidebar_rows: Vec::new(),
            collapsed_folders: HashSet::new(),
            sidebar_width: sidebar::SIDEBAR_WIDTH,
            sidebar_collapsed: false,
            drawer_height: response_drawer::DRAWER_HEIGHT,
            drawer_collapsed: false,
            overlay_selected: 0,
            overlay_error: None,
        };
        state.open_collection_at(cwd);
        state.view = View::Home;
        state
    }

    pub fn open_collection_at(&mut self, cwd: PathBuf) {
        let collection = collection::load(&cwd).map_err(|err| err.to_string());
        self.environment = collection
            .as_ref()
            .ok()
            .and_then(|c| c.default_environment.clone());
        self.active_file = collection
            .as_ref()
            .ok()
            .and_then(|c| c.all_requests().into_iter().next())
            .map(|r| ActiveFile::Request(r.abs_path.clone()))
            .unwrap_or(ActiveFile::None);
        self.open_tabs = match &self.active_file {
            ActiveFile::None => Vec::new(),
            file => vec![file.clone()],
        };
        if collection.is_ok() {
            let _ = epistola_engine::recent::record(&cwd);
        }
        self.cwd = cwd;
        self.collection = collection;
        self.collapsed_folders.clear();
        self.refresh_sidebar_rows();
        self.view = View::Workspace;
        self.close_overlay();
        self.activity.clear();
        self.response_subtab = ResponseSubTab::default();
        self.collection_action_error = None;
        self.recent_collections = epistola_engine::recent::list().unwrap_or_default();
        self.editor_buffers.clear();
        self.url_previews.clear();
        let active = self.active_file.clone();
        self.ensure_buffer(&active);
        if let ActiveFile::Request(path) = &active {
            self.refresh_url_preview(path);
        }
    }

    fn ensure_buffer(&mut self, file: &ActiveFile) {
        if self.editor_buffers.contains_key(file) {
            return;
        }
        let buffer = match file {
            ActiveFile::None => return,
            ActiveFile::Config => {
                let vars = epistola_format::load_global_config().unwrap_or_default();
                let text = if vars.is_empty() {
                    "# no global variables set yet".to_string()
                } else {
                    let mut lines = vec!["[variables]".to_string()];
                    lines.extend(vars.iter().map(|(k, v)| format!("{k} = \"{v}\"")));
                    lines.join("\n")
                };
                EditorBuffer::read_only(text)
            }
            _ => {
                let Some(path) = file.disk_path(self.collection.as_ref().ok()) else {
                    return;
                };
                match std::fs::read_to_string(&path) {
                    Ok(text) => EditorBuffer::new(text),
                    Err(_) => EditorBuffer::read_only(format!("Could not read {}", path.display())),
                }
            }
        };
        self.editor_buffers.insert(file.clone(), buffer);
    }

    pub fn refresh_url_preview(&mut self, path: &Path) {
        let raw = std::fs::read_to_string(path).ok();
        let virtual_note = raw.as_deref().and_then(|raw| {
            let collection = self.collection.as_ref().ok()?;
            let rel_dir = path.strip_prefix(&collection.root).ok()?.parent()?;
            let file = RequestFile::from_toml_str(raw).ok()?;
            if file.request.auth.is_some() {
                return None;
            }
            nearest_folder_auth(&collection.root, rel_dir)
        });

        let preview = match epistola_engine::run::resolve_saved_request(
            path,
            self.environment.as_deref(),
            Default::default(),
        ) {
            Ok((_collection, resolved)) => UrlPreview {
                text: resolved.request.url.clone(),
                unresolved_variable: None,
                virtual_note,
            },
            Err(engine_err) => {
                let raw_url = raw
                    .as_deref()
                    .and_then(|raw| RequestFile::from_toml_str(raw).ok())
                    .map(|file| file.request.url)
                    .unwrap_or_default();
                UrlPreview {
                    text: raw_url,
                    unresolved_variable: execution::unresolved_variable(&engine_err),
                    virtual_note,
                }
            }
        };
        self.url_previews.insert(path.to_path_buf(), preview);
    }

    fn refresh_all_url_previews(&mut self) {
        let paths: Vec<PathBuf> = self
            .open_tabs
            .iter()
            .filter_map(|file| match file {
                ActiveFile::Request(path) => Some(path.clone()),
                _ => None,
            })
            .collect();
        for path in paths {
            self.refresh_url_preview(&path);
        }
    }

    fn open_tab(&mut self, file: ActiveFile) {
        if !self.open_tabs.contains(&file) {
            self.open_tabs.push(file.clone());
        }
        self.ensure_buffer(&file);
        if let ActiveFile::Request(path) = &file {
            self.refresh_url_preview(path);
        }
        self.active_file = file;
        self.view = View::Workspace;
        self.overlay = None;
    }

    pub fn open_request(&mut self, path: PathBuf) {
        self.open_tab(ActiveFile::Request(path));
    }

    pub fn open_config(&mut self) {
        self.open_tab(ActiveFile::Config);
    }

    pub fn open_folder_doc(&mut self, folder_dir: PathBuf) {
        self.open_tab(ActiveFile::Folder(folder_dir));
    }

    pub fn open_environment_doc(&mut self, name: String) {
        self.open_tab(ActiveFile::Environment(name));
    }

    pub fn close_tab(&mut self, file: &ActiveFile) {
        let Some(idx) = self.open_tabs.iter().position(|f| f == file) else {
            return;
        };
        self.open_tabs.remove(idx);
        self.activity.remove(file);
        self.editor_buffers.remove(file);
        if &self.active_file == file {
            self.active_file = self
                .open_tabs
                .get(idx.min(self.open_tabs.len().saturating_sub(1)))
                .cloned()
                .unwrap_or(ActiveFile::None);
        }
    }

    pub fn switch_tab(&mut self, file: ActiveFile) {
        if self.open_tabs.contains(&file) {
            self.active_file = file;
        }
    }

    pub fn active_request(&self) -> Option<&RequestEntry> {
        match &self.active_file {
            ActiveFile::Request(path) => self
                .collection
                .as_ref()
                .ok()
                .and_then(|c| c.find_request(path)),
            _ => None,
        }
    }

    pub fn active_activity(&self) -> &ActivityResult {
        self.activity
            .get(&self.active_file)
            .unwrap_or(&ActivityResult::Idle)
    }

    pub fn active_buffer(&self) -> Option<&EditorBuffer> {
        self.editor_buffers.get(&self.active_file)
    }

    pub fn active_buffer_mut(&mut self) -> Option<&mut EditorBuffer> {
        self.editor_buffers.get_mut(&self.active_file)
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.editor_buffers.values().any(EditorBuffer::is_dirty)
    }

    pub fn is_dirty(&self, file: &ActiveFile) -> bool {
        self.editor_buffers
            .get(file)
            .is_some_and(EditorBuffer::is_dirty)
    }

    /// A no-op if the collection failed to load or defines no environments.
    pub fn cycle_environment(&mut self) {
        let Ok(collection) = self.collection.as_ref() else {
            return;
        };
        if collection.environments.is_empty() {
            self.environment = None;
            return;
        }
        let next = match &self.environment {
            Some(current) => {
                let idx = collection
                    .environments
                    .iter()
                    .position(|name| name == current)
                    .unwrap_or(0);
                collection.environments[(idx + 1) % collection.environments.len()].clone()
            }
            None => collection.environments[0].clone(),
        };
        self.environment = Some(next);
        self.refresh_all_url_previews();
    }

    pub fn set_environment(&mut self, name: String) {
        self.environment = Some(name);
        self.refresh_all_url_previews();
    }

    pub fn overlay_selection(&self, len: usize) -> usize {
        self.overlay_selected.min(len.saturating_sub(1))
    }

    pub fn open_overlay(&mut self, overlay: Overlay) {
        self.overlay = Some(overlay);
        self.overlay_selected = 0;
        self.overlay_error = None;
    }

    pub fn close_overlay(&mut self) {
        self.overlay = None;
        self.overlay_selected = 0;
        self.overlay_error = None;
        self.history_entries.clear();
    }

    /// Reloads the sidebar's `CollectionTree` after a request CRUD operation,
    /// without touching open tabs/buffers/active file the way
    /// `open_collection_at` does.
    pub fn refresh_collection(&mut self) {
        self.collection = collection::load(&self.cwd).map_err(|err| err.to_string());
        self.refresh_sidebar_rows();
    }

    fn refresh_sidebar_rows(&mut self) {
        self.sidebar_rows = sidebar::flatten(&self.collection, &self.collapsed_folders);
    }

    /// Toggles whether `dir`'s children are hidden in the sidebar. Session-only:
    /// reopening the collection resets it, same as any other tree navigation state.
    pub fn toggle_folder_collapse(&mut self, dir: PathBuf) {
        if !self.collapsed_folders.remove(&dir) {
            self.collapsed_folders.insert(dir);
        }
        self.refresh_sidebar_rows();
    }

    /// Points the tab (and any activity/buffer) that held `old` at `new_path`
    /// instead, preserving which tab is active.
    pub fn replace_request_tab(&mut self, old: &Path, new_path: PathBuf) {
        let old_file = ActiveFile::Request(old.to_path_buf());
        let new_file = ActiveFile::Request(new_path);

        if let Some(slot) = self.open_tabs.iter_mut().find(|f| **f == old_file) {
            *slot = new_file.clone();
        }
        self.editor_buffers.remove(&old_file);
        if let Some(activity) = self.activity.remove(&old_file) {
            self.activity.insert(new_file.clone(), activity);
        }
        if self.active_file == old_file {
            self.active_file = new_file.clone();
        }
        self.ensure_buffer(&new_file);
        self.url_previews.remove(old);
        if let ActiveFile::Request(path) = &new_file {
            self.refresh_url_preview(path);
        }
    }

    /// Closes the tab for `path` if it's open — used after a delete, where
    /// there's no file left to prompt "unsaved changes" about.
    pub fn close_request_tab_if_open(&mut self, path: &Path) {
        self.close_tab(&ActiveFile::Request(path.to_path_buf()));
    }
}

fn nearest_folder_auth(collection_root: &Path, request_rel_dir: &Path) -> Option<String> {
    let mut dir = request_rel_dir.to_path_buf();
    loop {
        let candidate = collection_root.join(&dir).join("folder.toml");
        if candidate.is_file() {
            if let Ok(manifest) = FolderManifest::load(&candidate) {
                if manifest.auth.is_some() {
                    return Some(if dir.as_os_str().is_empty() {
                        "folder.toml".to_string()
                    } else {
                        format!("{}/folder.toml", dir.display())
                    });
                }
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}
