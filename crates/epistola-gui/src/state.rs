use std::collections::HashMap;
use std::path::PathBuf;

use crate::buffer::EditorBuffer;
use crate::collection::{self, CollectionTree, RequestEntry};

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
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Overlay {
    CommandPalette,
    QuickOpen,
    EnvironmentPicker,
    History,
    ConfirmDiscard(ConfirmDiscardKind),
}

/// `CloseTab` offers Save (one unambiguous file); `SwitchCollection` only
/// offers Discard (possibly many dirty tabs, no single file to save).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ConfirmDiscardKind {
    CloseTab(ActiveFile),
    SwitchCollection(PathBuf),
}

#[derive(Clone, Debug)]
pub enum ActivityResult {
    Idle,
    Running,
    RunSuccess {
        status: u16,
        duration_ms: u128,
        content_length: usize,
        body: String,
        headers: Vec<(String, String)>,
    },
    RunFailed(String),
    UnresolvedVariable {
        variable: String,
    },
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
        self.view = View::Workspace;
        self.overlay = None;
        self.activity.clear();
        self.response_subtab = ResponseSubTab::default();
        self.collection_action_error = None;
        self.recent_collections = epistola_engine::recent::list().unwrap_or_default();
        self.editor_buffers.clear();
        let active = self.active_file.clone();
        self.ensure_buffer(&active);
    }

    fn load_buffer_text(&self, file: &ActiveFile) -> Option<String> {
        match file {
            ActiveFile::Request(path) => std::fs::read_to_string(path).ok(),
            ActiveFile::Folder(dir) => std::fs::read_to_string(dir.join("folder.toml")).ok(),
            ActiveFile::Environment(name) => {
                let collection = self.collection.as_ref().ok()?;
                std::fs::read_to_string(
                    collection
                        .root
                        .join("environments")
                        .join(format!("{name}.toml")),
                )
                .ok()
            }
            ActiveFile::Config | ActiveFile::None => None,
        }
    }

    fn ensure_buffer(&mut self, file: &ActiveFile) {
        if self.editor_buffers.contains_key(file) {
            return;
        }
        if let Some(text) = self.load_buffer_text(file) {
            self.editor_buffers
                .insert(file.clone(), EditorBuffer::new(text));
        }
    }

    fn open_tab(&mut self, file: ActiveFile) {
        if !self.open_tabs.contains(&file) {
            self.open_tabs.push(file.clone());
        }
        self.ensure_buffer(&file);
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
    }

    pub fn set_environment(&mut self, name: String) {
        self.environment = Some(name);
    }
}
