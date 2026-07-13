use std::path::PathBuf;

use crate::collection::{self, CollectionTree, RequestEntry};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Home,
    Workspace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActiveFile {
    None,
    Request(PathBuf),
    Config,
}

impl ActiveFile {
    pub fn is_request(&self) -> bool {
        matches!(self, ActiveFile::Request(_))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Overlay {
    CommandPalette,
    QuickOpen,
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
    },
    RunFailed(String),
    Resolved(String),
    ResolvedFailed(String),
    Linted(String),
    LintFailed(String),
}

pub struct AppState {
    pub cwd: PathBuf,
    pub view: View,
    pub active_file: ActiveFile,
    pub environment: Option<String>,
    pub collection: Result<CollectionTree, String>,
    pub overlay: Option<Overlay>,
    pub activity: ActivityResult,
    pub collection_action_error: Option<String>,
    pub recent_collections: Vec<PathBuf>,
}

impl AppState {
    pub fn new(cwd: PathBuf) -> Self {
        let mut state = Self {
            cwd: PathBuf::new(),
            view: View::Workspace,
            active_file: ActiveFile::None,
            environment: None,
            collection: Err(String::new()),
            overlay: None,
            activity: ActivityResult::Idle,
            collection_action_error: None,
            recent_collections: Vec::new(),
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
        if collection.is_ok() {
            let _ = epistola_engine::recent::record(&cwd);
        }
        self.cwd = cwd;
        self.collection = collection;
        self.view = View::Workspace;
        self.overlay = None;
        self.activity = ActivityResult::Idle;
        self.collection_action_error = None;
        self.recent_collections = epistola_engine::recent::list().unwrap_or_default();
    }

    pub fn open_request(&mut self, path: PathBuf) {
        self.active_file = ActiveFile::Request(path);
        self.view = View::Workspace;
        self.overlay = None;
        self.activity = ActivityResult::Idle;
    }

    pub fn open_config(&mut self) {
        self.active_file = ActiveFile::Config;
        self.view = View::Workspace;
        self.overlay = None;
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
}
