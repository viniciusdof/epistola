use gpui::{Context, Focusable, KeyDownEvent, ScrollStrategy, UniformListScrollHandle, Window};

use crate::actions::{
    ClearCookies, CycleEnvironment, DeleteRequest, DuplicateRequest, InstallCli, LintCollection,
    NewCollection, NewEnvironment, NewFolder, NewRequest, OpenAbout, OpenCollection,
    OpenEnvironmentDoc, OpenRecentCollection, OpenRequestFile, OpenSettings, RenameRequest,
    RunActiveRequest, SelectEnvironment, ShowResolvedRequest,
};
use crate::components::history_modal::filter_history;
use crate::components::picker::{filter_items, PickerItem};
use crate::root::EpistolaGui;
use crate::state::{AppState, Overlay};

impl EpistolaGui {
    /// Opens an overlay whose query/value is typed into the shared `overlay_input` field.
    pub(crate) fn open_text_overlay(
        &mut self,
        overlay: Overlay,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.open_overlay(overlay);
        self.overlay_scroll = UniformListScrollHandle::default();
        self.overlay_input.update(cx, |field, cx| {
            field.set_placeholder(placeholder.to_string(), cx);
            field.clear(cx);
        });
        window.focus(&self.overlay_input.focus_handle(cx), cx);
        self.refresh_overlay_items(cx);
        cx.notify();
    }

    pub(crate) fn toggle_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.overlay == Some(Overlay::CommandPalette) {
            self.close_overlay(window, cx);
        } else {
            self.open_text_overlay(Overlay::CommandPalette, "Type a command…", window, cx);
        }
    }

    pub(crate) fn toggle_quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.overlay == Some(Overlay::QuickOpen) {
            self.close_overlay(window, cx);
        } else {
            self.open_text_overlay(Overlay::QuickOpen, "Go to request…", window, cx);
        }
    }

    pub(crate) fn open_environment_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_text_overlay(
            Overlay::EnvironmentPicker,
            "Filter environments…",
            window,
            cx,
        );
    }

    pub(crate) fn open_collection_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_text_overlay(Overlay::SwitchCollection, "Search collections…", window, cx);
    }

    pub(crate) fn open_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.history_entries = self
            .state
            .collection()
            .map(|collection| {
                epistola_engine::history::read_entries(&collection.root).unwrap_or_default()
            })
            .unwrap_or_default();
        self.open_text_overlay(Overlay::History, "Filter history…", window, cx);
    }

    pub(crate) fn open_about(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.open_overlay(Overlay::About);
        cx.notify();
    }

    pub(crate) fn refresh_overlay_items(&mut self, cx: &mut Context<Self>) {
        let items = match self.state.overlay {
            Some(Overlay::CommandPalette) => command_palette_items(&self.state),
            Some(Overlay::QuickOpen) => quick_open_items(&self.state),
            Some(Overlay::EnvironmentPicker) => environment_picker_items(&self.state),
            Some(Overlay::SwitchCollection) => switch_collection_items(&self.state),
            _ => Vec::new(),
        };
        let query = self.overlay_input.read(cx).text().to_string();
        self.overlay_items = filter_items(&mut self.overlay_matcher, items, &query);
    }

    pub(crate) fn select_environment(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.set_environment(name);
        self.close_overlay(window, cx);
    }

    pub(crate) fn overlay_item_count(&self, cx: &Context<Self>) -> usize {
        match self.state.overlay {
            Some(Overlay::CommandPalette)
            | Some(Overlay::QuickOpen)
            | Some(Overlay::EnvironmentPicker)
            | Some(Overlay::SwitchCollection) => self.overlay_items.len(),
            Some(Overlay::History) => {
                let query = self.overlay_input.read(cx).text().to_string();
                filter_history(&self.state.history_entries, &query).len()
            }
            Some(Overlay::About) | Some(Overlay::Prompt(_)) | None => 0,
        }
    }

    pub(crate) fn move_overlay_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.overlay_item_count(cx);
        if count == 0 {
            return;
        }
        let count = count as isize;
        let current = self.state.overlay_selected as isize;
        let next = ((current + delta) % count + count) % count;
        self.state.overlay_selected = next as usize;
        self.overlay_scroll
            .scroll_to_item(next as usize, ScrollStrategy::Top);
        cx.notify();
    }

    pub(crate) fn confirm_overlay_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(overlay) = self.state.overlay.clone() else {
            return;
        };
        let selected = self.state.overlay_selected;
        match overlay {
            Overlay::CommandPalette
            | Overlay::QuickOpen
            | Overlay::EnvironmentPicker
            | Overlay::SwitchCollection => {
                if let Some(item) = self.overlay_items.get(selected) {
                    let action = item.action.boxed_clone();
                    if item.closes_overlay {
                        self.close_overlay(window, cx);
                    }
                    window.dispatch_action(action, cx);
                }
            }
            Overlay::Prompt(kind) => self.submit_prompt(kind, window, cx),
            Overlay::History | Overlay::About => {}
        }
    }

    pub(crate) fn handle_overlay_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.overlay.is_none() {
            return;
        }
        match event.keystroke.key.as_str() {
            "down" => self.move_overlay_selection(1, cx),
            "up" => self.move_overlay_selection(-1, cx),
            "enter" => self.confirm_overlay_selection(window, cx),
            _ => {}
        }
    }

    pub(crate) fn dismiss_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.overlay.is_some() {
            self.close_overlay(window, cx);
        }
    }
}

fn command_palette_items(state: &AppState) -> Vec<PickerItem> {
    let mut items = Vec::new();

    if state.active_request().is_some() {
        items.push(PickerItem::new("Run request", RunActiveRequest).detail("⌘⏎"));
        items.push(PickerItem::new("Show resolved request", ShowResolvedRequest).detail("⌘⇧R"));
        items.push(PickerItem::new("Rename Request", RenameRequest).keep_overlay_open());
        items.push(PickerItem::new("Duplicate Request", DuplicateRequest).keep_overlay_open());
        items.push(PickerItem::new("Delete Request", DeleteRequest).keep_overlay_open());
    }

    if state.collection().is_some() {
        items.push(PickerItem::new("New Request", NewRequest).keep_overlay_open());
        items.push(PickerItem::new("New Folder", NewFolder).keep_overlay_open());
        items.push(PickerItem::new("New Environment", NewEnvironment).keep_overlay_open());
        items.push(PickerItem::new("Lint collection", LintCollection).detail("⌘⇧L"));
        items.push(PickerItem::new("Switch environment", CycleEnvironment).detail("⌘E"));
        items.push(PickerItem::new("Clear Cookies", ClearCookies));
    }

    items.push(PickerItem::new("Open settings", OpenSettings).detail("⌘,"));
    items.push(PickerItem::new("Install CLI", InstallCli));
    items.push(PickerItem::new("About Epistola", OpenAbout));

    items
}

fn quick_open_items(state: &AppState) -> Vec<PickerItem> {
    let Some(collection) = state.collection() else {
        return Vec::new();
    };
    let mut items: Vec<PickerItem> = collection
        .all_requests()
        .into_iter()
        .map(|request| {
            let path = request.abs_path.clone();
            PickerItem::new(
                request.rel_path.display().to_string(),
                OpenRequestFile { path },
            )
            .leading_method(request.method.clone())
        })
        .collect();
    items.extend(collection.environments.iter().map(|name| {
        PickerItem::new(
            format!("environments/{name}.toml"),
            OpenEnvironmentDoc { name: name.clone() },
        )
    }));
    items
}

fn switch_collection_items(state: &AppState) -> Vec<PickerItem> {
    let mut items: Vec<PickerItem> = state
        .recent_collections
        .iter()
        .map(|path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let is_current = path == &state.cwd;
            PickerItem::new(name, OpenRecentCollection { path: path.clone() })
                .leading_dot(is_current)
                .detail(path.display().to_string())
        })
        .collect();
    items.push(PickerItem::new("Open Folder…", OpenCollection));
    items.push(PickerItem::new("New Collection…", NewCollection));
    items
}

fn environment_picker_items(state: &AppState) -> Vec<PickerItem> {
    let Some(collection) = state.collection() else {
        return Vec::new();
    };
    collection
        .environments
        .iter()
        .map(|name| {
            let active = state.environment.as_deref() == Some(name.as_str());
            PickerItem::new(name.clone(), SelectEnvironment { name: name.clone() })
                .leading_dot(active)
                .detail(format!("{name}.toml"))
        })
        .collect()
}
