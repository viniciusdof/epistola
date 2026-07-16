use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use epistola_core::Response;
use epistola_engine::history::HistoryEntry;
use epistola_format::{FolderManifest, RequestFile};
use gpui::Pixels;

use crate::buffer::EditorBuffer;
use crate::collection::{CollectionTree, RequestEntry};
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
    SwitchCollection,
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

    pub collection: Option<Result<CollectionTree, String>>,
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
            view: View::Home,
            active_file: ActiveFile::None,
            open_tabs: Vec::new(),
            environment: None,
            collection: None,
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
        state.begin_open_collection(cwd);
        state
    }

    /// Clears the previous collection's UI state and marks `collection` as
    /// loading until `apply_opened_collection` delivers the result.
    pub fn begin_open_collection(&mut self, cwd: PathBuf) {
        self.cwd = cwd;
        self.collection = None;
        self.environment = None;
        self.active_file = ActiveFile::None;
        self.open_tabs = Vec::new();
        self.collapsed_folders.clear();
        self.view = View::Home;
        self.close_overlay();
        self.activity.clear();
        self.response_subtab = ResponseSubTab::default();
        self.collection_action_error = None;
        self.recent_collections = epistola_engine::recent::list().unwrap_or_default();
        self.editor_buffers.clear();
        self.url_previews.clear();
        self.refresh_sidebar_rows();
    }

    /// Ignored if `cwd` no longer matches `self.cwd` (user switched again mid-load).
    pub fn apply_opened_collection(
        &mut self,
        cwd: PathBuf,
        result: Result<CollectionTree, String>,
    ) {
        if cwd != self.cwd {
            return;
        }
        self.environment = result
            .as_ref()
            .ok()
            .and_then(|c| c.default_environment.clone());
        self.active_file = result
            .as_ref()
            .ok()
            .and_then(|c| c.all_requests().into_iter().next())
            .map(|r| ActiveFile::Request(r.abs_path.clone()))
            .unwrap_or(ActiveFile::None);
        self.open_tabs = match &self.active_file {
            ActiveFile::None => Vec::new(),
            file => vec![file.clone()],
        };
        self.view = if result.is_ok() {
            let _ = epistola_engine::recent::record(&cwd);
            View::Workspace
        } else {
            View::Home
        };
        self.collection = Some(result);
        self.refresh_sidebar_rows();
        let active = self.active_file.clone();
        self.ensure_buffer(&active);
        if let ActiveFile::Request(path) = &active {
            self.refresh_url_preview(path);
        }
    }

    /// `None` both while loading and after a load failure.
    pub fn collection(&self) -> Option<&CollectionTree> {
        self.collection.as_ref()?.as_ref().ok()
    }

    /// Distinct from `None`, which also covers "still loading".
    pub fn collection_error(&self) -> Option<&str> {
        self.collection.as_ref()?.as_ref().err().map(String::as_str)
    }

    pub fn is_collection_loading(&self) -> bool {
        self.collection.is_none()
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
                let Some(path) = file.disk_path(self.collection()) else {
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
            let collection = self.collection()?;
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
            ActiveFile::Request(path) => self.collection().and_then(|c| c.find_request(path)),
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

    /// A no-op if the collection isn't loaded or defines no environments.
    pub fn cycle_environment(&mut self) {
        let Some(collection) = self.collection() else {
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

    pub fn apply_refreshed_collection(&mut self, result: Result<CollectionTree, String>) {
        self.collection = Some(result);
        self.refresh_sidebar_rows();
    }

    fn refresh_sidebar_rows(&mut self) {
        let Some(collection) = self.collection.as_ref() else {
            self.sidebar_rows = Vec::new();
            return;
        };
        self.sidebar_rows = sidebar::flatten(collection, &self.collapsed_folders);
    }

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

    /// Reacts to a debounced batch of externally-touched paths under the open collection.
    pub fn handle_fs_events(&mut self, paths: &[PathBuf]) {
        if self.collection().is_none() {
            return;
        }
        let touched: HashSet<PathBuf> = paths.iter().cloned().collect();

        let files: Vec<ActiveFile> = self.editor_buffers.keys().cloned().collect();
        for file in files {
            let Some(disk_path) = file.disk_path(self.collection()) else {
                continue;
            };
            if !touched.contains(&disk_path) {
                continue;
            }
            if self.is_dirty(&file) {
                if let Some(buffer) = self.editor_buffers.get_mut(&file) {
                    buffer.external_change = true;
                }
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&disk_path) else {
                continue;
            };
            let Some(buffer) = self.editor_buffers.get(&file) else {
                continue;
            };
            // Same text is either the app's own write echoing back through the
            // watcher, or a no-op external touch — reloading would wipe undo
            // history for nothing.
            if buffer.text == text {
                continue;
            }
            if let Some(buffer) = self.editor_buffers.get_mut(&file) {
                buffer.set_text(text);
                buffer.mark_saved();
            }
            if let ActiveFile::Request(path) = &file {
                self.refresh_url_preview(path);
            }
        }
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;
    use crate::collection::CollectionTree;

    fn state_with_collection(root: PathBuf) -> AppState {
        AppState {
            cwd: root.clone(),
            view: View::Workspace,
            active_file: ActiveFile::None,
            open_tabs: Vec::new(),
            environment: None,
            collection: Some(Ok(CollectionTree {
                root,
                name: "test".to_string(),
                folders: Vec::new(),
                requests: Vec::new(),
                environments: Vec::new(),
                default_environment: None,
                index: Default::default(),
            })),
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
        }
    }

    #[test]
    fn clean_buffer_reloads_when_file_changes_on_disk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        std::fs::write(&path, "new content").unwrap();

        let mut state = state_with_collection(dir.path().to_path_buf());
        let file = ActiveFile::Request(path.clone());
        state
            .editor_buffers
            .insert(file.clone(), EditorBuffer::new("old content".to_string()));

        state.handle_fs_events(std::slice::from_ref(&path));

        assert_eq!(state.editor_buffers[&file].text, "new content");
        assert!(!state.editor_buffers[&file].is_dirty());
    }

    #[test]
    fn dirty_buffer_flags_external_change_without_overwriting() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        std::fs::write(&path, "disk content").unwrap();

        let mut state = state_with_collection(dir.path().to_path_buf());
        let file = ActiveFile::Request(path.clone());
        let mut buffer = EditorBuffer::new("original".to_string());
        buffer.replace_range(0..0, "unsaved edit ");
        state.editor_buffers.insert(file.clone(), buffer);

        state.handle_fs_events(std::slice::from_ref(&path));

        let buffer = &state.editor_buffers[&file];
        assert!(buffer.text.starts_with("unsaved edit"));
        assert!(buffer.external_change);
    }

    #[test]
    fn matching_text_is_a_no_op_and_preserves_undo_history() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");

        let mut buffer = EditorBuffer::new("abc".to_string());
        buffer.replace_range(3..3, "d");
        buffer.mark_saved();
        std::fs::write(&path, &buffer.text).unwrap();

        let mut state = state_with_collection(dir.path().to_path_buf());
        let file = ActiveFile::Request(path.clone());
        state.editor_buffers.insert(file.clone(), buffer);

        state.handle_fs_events(std::slice::from_ref(&path));

        let buffer = state.editor_buffers.get_mut(&file).unwrap();
        buffer.undo();
        assert_eq!(buffer.text, "abc");
    }

    #[test]
    fn untouched_path_is_ignored() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        std::fs::write(&path, "disk").unwrap();
        let other = dir.path().join("other.req.toml");

        let mut state = state_with_collection(dir.path().to_path_buf());
        let file = ActiveFile::Request(path.clone());
        state
            .editor_buffers
            .insert(file.clone(), EditorBuffer::new("buffer text".to_string()));

        state.handle_fs_events(&[other]);

        assert_eq!(state.editor_buffers[&file].text, "buffer text");
    }

    fn empty_collection(root: PathBuf) -> CollectionTree {
        CollectionTree {
            root,
            name: "test".to_string(),
            folders: Vec::new(),
            requests: Vec::new(),
            environments: Vec::new(),
            default_environment: None,
            index: Default::default(),
        }
    }

    #[test]
    fn begin_open_collection_marks_loading_and_resets_ui_state() {
        let dir = tempdir().unwrap();
        let mut state = state_with_collection(dir.path().to_path_buf());
        state.open_tabs.push(ActiveFile::Config);
        state.active_file = ActiveFile::Config;

        state.begin_open_collection(dir.path().to_path_buf());

        assert!(state.collection.is_none());
        assert!(state.is_collection_loading());
        assert!(state.collection().is_none());
        assert!(state.open_tabs.is_empty());
        assert_eq!(state.view, View::Home);
    }

    #[test]
    fn apply_opened_collection_ignores_a_stale_cwd() {
        let dir = tempdir().unwrap();
        let mut state = state_with_collection(dir.path().to_path_buf());
        state.begin_open_collection(dir.path().to_path_buf());

        let other_dir = tempdir().unwrap();
        // The user switched again before the first load for `dir` came back.
        state.begin_open_collection(other_dir.path().to_path_buf());
        state.apply_opened_collection(
            dir.path().to_path_buf(),
            Ok(empty_collection(dir.path().to_path_buf())),
        );

        assert!(state.is_collection_loading());
        assert_eq!(state.cwd, other_dir.path());
    }

    #[test]
    fn apply_opened_collection_success_switches_to_workspace() {
        let dir = tempdir().unwrap();
        let mut state = state_with_collection(dir.path().to_path_buf());
        state.begin_open_collection(dir.path().to_path_buf());

        state.apply_opened_collection(
            dir.path().to_path_buf(),
            Ok(empty_collection(dir.path().to_path_buf())),
        );

        assert_eq!(state.view, View::Workspace);
        assert!(state.collection().is_some());
    }

    #[test]
    fn apply_opened_collection_failure_stays_on_home() {
        let dir = tempdir().unwrap();
        let mut state = state_with_collection(dir.path().to_path_buf());
        state.begin_open_collection(dir.path().to_path_buf());

        state.apply_opened_collection(dir.path().to_path_buf(), Err("boom".to_string()));

        assert_eq!(state.view, View::Home);
        assert_eq!(state.collection_error(), Some("boom"));
    }

    #[test]
    fn apply_refreshed_collection_leaves_tabs_and_active_file_untouched() {
        let dir = tempdir().unwrap();
        let mut state = state_with_collection(dir.path().to_path_buf());
        let file = ActiveFile::Config;
        state.open_tabs.push(file.clone());
        state.active_file = file.clone();

        state.apply_refreshed_collection(Ok(empty_collection(dir.path().to_path_buf())));

        assert_eq!(state.open_tabs, vec![file.clone()]);
        assert_eq!(state.active_file, file);
        assert!(state.collection().is_some());
    }
}
