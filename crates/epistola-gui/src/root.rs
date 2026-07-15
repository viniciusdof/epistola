//! The window's single root view. It owns `AppState` and is the only place
//! that mutates it.

use std::path::PathBuf;

use gpui::{
    div, prelude::*, px, App, ClickEvent, Context, Entity, FocusHandle, Focusable, IntoElement,
    KeyDownEvent, MouseButton, PromptLevel, Render, ScrollHandle, ScrollStrategy,
    UniformListScrollHandle, Window,
};
use nucleo_matcher::{Config, Matcher};

use crate::actions::{
    CloseTab, CycleEnvironment, DeleteRequest, Dismiss, DuplicateRequest, GoHome, GoWorkspace,
    LintCollection, NewCollection, NewRequest, OpenCollection, OpenEnvironmentDoc,
    OpenEnvironmentPicker, OpenFolderDoc, OpenHistory, OpenRecentCollection, OpenRequestFile,
    OpenSettings, RenameRequest, RunActiveRequest, SelectEnvironment, SelectResponseSubtab,
    ShowResolvedRequest, SwitchTab, ToggleCommandPalette, ToggleDrawer, ToggleFolderCollapse,
    ToggleQuickOpen, ToggleSidebar,
};
use crate::components::editor_text::EditorTextLayout;
use crate::components::history_modal;
use crate::components::picker::{filter_items, render_picker, PickerItem};
use crate::components::prompt_modal::render_prompt_modal;
use crate::components::{
    activity_rail, editor, home, response_drawer, sidebar, statusbar, titlebar,
};
use crate::editor_save;
use crate::execution;
use crate::state::{ActiveFile, ActivityResult, AppState, Overlay, PromptKind, View};
use crate::text_field::TextField;
use crate::theme::Theme;

/// Which panel a drag-in-progress is resizing. Window interaction, not
/// domain state — lives here next to `editor_mouse_selecting`, not on
/// `AppState`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResizingPanel {
    Sidebar,
    Drawer,
}

pub struct EpistolaGui {
    pub(crate) state: AppState,
    pub(crate) editor_focus_handle: FocusHandle,
    overlay_focus_handle: FocusHandle,
    overlay_input: Entity<TextField>,
    overlay_scroll: UniformListScrollHandle,
    pub(crate) editor_layout: Option<EditorTextLayout>,
    pub(crate) editor_mouse_selecting: bool,
    editor_scroll_handle: ScrollHandle,
    overlay_items: Vec<PickerItem>,
    overlay_matcher: Matcher,
    pub(crate) resizing: Option<ResizingPanel>,
}

impl EpistolaGui {
    pub fn new(cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        let overlay_input = cx.new(|cx| TextField::new("", cx));
        cx.observe(&overlay_input, |this, _field, cx| {
            this.refresh_overlay_items(cx);
            cx.notify();
        })
        .detach();

        Self {
            state: AppState::new(cwd),
            editor_focus_handle: cx.focus_handle(),
            overlay_focus_handle: cx.focus_handle(),
            overlay_input,
            overlay_scroll: UniformListScrollHandle::default(),
            editor_layout: None,
            editor_mouse_selecting: false,
            editor_scroll_handle: ScrollHandle::new(),
            overlay_items: Vec::new(),
            overlay_matcher: Matcher::new(Config::DEFAULT),
            resizing: None,
        }
    }
}

impl Focusable for EpistolaGui {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.editor_focus_handle.clone()
    }
}

impl EpistolaGui {
    fn open_overlay(&mut self, overlay: Overlay, window: &mut Window, cx: &mut Context<Self>) {
        self.state.open_overlay(overlay);
        self.overlay_scroll = UniformListScrollHandle::default();
        window.focus(&self.overlay_focus_handle, cx);
        cx.notify();
    }

    /// Opens an overlay whose query/value is typed into the shared `overlay_input` field.
    fn open_text_overlay(
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

    pub(crate) fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.close_overlay();
        window.focus(&self.editor_focus_handle, cx);
        cx.notify();
    }

    fn toggle_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.overlay == Some(Overlay::CommandPalette) {
            self.close_overlay(window, cx);
        } else {
            self.open_text_overlay(Overlay::CommandPalette, "Type a command…", window, cx);
        }
    }

    fn toggle_quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.overlay == Some(Overlay::QuickOpen) {
            self.close_overlay(window, cx);
        } else {
            self.open_text_overlay(Overlay::QuickOpen, "Go to request…", window, cx);
        }
    }

    fn open_environment_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_text_overlay(
            Overlay::EnvironmentPicker,
            "Filter environments…",
            window,
            cx,
        );
    }

    fn open_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.history_entries = self
            .state
            .collection
            .as_ref()
            .ok()
            .map(|collection| {
                epistola_engine::history::read_entries(&collection.root).unwrap_or_default()
            })
            .unwrap_or_default();
        self.open_overlay(Overlay::History, window, cx);
    }

    fn refresh_overlay_items(&mut self, cx: &mut Context<Self>) {
        let items = match self.state.overlay {
            Some(Overlay::CommandPalette) => command_palette_items(&self.state),
            Some(Overlay::QuickOpen) => quick_open_items(&self.state),
            Some(Overlay::EnvironmentPicker) => environment_picker_items(&self.state),
            _ => Vec::new(),
        };
        let query = self.overlay_input.read(cx).text().to_string();
        self.overlay_items = filter_items(&mut self.overlay_matcher, items, &query);
    }

    fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.state.open_config();
        cx.notify();
    }

    fn go_home(&mut self, cx: &mut Context<Self>) {
        self.state.view = View::Home;
        cx.notify();
    }

    fn go_workspace(&mut self, cx: &mut Context<Self>) {
        self.state.view = View::Workspace;
        cx.notify();
    }

    fn run_active_request(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.state.active_request().map(|r| r.abs_path.clone()) else {
            return;
        };
        execution::spawn_run(path, self.state.environment.clone(), cx);
    }

    fn show_resolved_request(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.state.active_request().map(|r| r.abs_path.clone()) else {
            return;
        };
        let tab = self.state.active_file.clone();
        let outcome = epistola_engine::run::resolve_saved_request(
            &path,
            self.state.environment.as_deref(),
            Default::default(),
        );
        let activity = match outcome {
            Ok((_collection, resolved)) => ActivityResult::Resolved(
                epistola_engine::output::format_request_text(&resolved.request),
            ),
            Err(engine_err) => match execution::classify_engine_error(engine_err) {
                ActivityResult::UnresolvedVariable { variable } => {
                    ActivityResult::UnresolvedVariable { variable }
                }
                ActivityResult::RunFailed(message) => ActivityResult::ResolvedFailed(message),
                other => other,
            },
        };
        self.state.activity.insert(tab, activity);
        cx.notify();
    }

    fn lint_collection(&mut self, cx: &mut Context<Self>) {
        if self.state.collection.is_err() {
            return;
        }
        let tab = self.state.active_file.clone();
        let result = epistola_engine::discovery::discover_collection(&self.state.cwd)
            .and_then(|collection| {
                epistola_engine::requests::lint_collection(
                    &collection,
                    self.state.environment.as_deref(),
                )
            })
            .map(|report| format_lint_report(&report))
            .map_err(|err| err.to_string());
        let activity = match result {
            Ok(text) => ActivityResult::Linted(text),
            Err(err) => ActivityResult::LintFailed(err),
        };
        self.state.activity.insert(tab, activity);
        cx.notify();
    }

    fn cycle_environment_action(&mut self, cx: &mut Context<Self>) {
        if self.state.collection.is_err() {
            return;
        }
        self.state.cycle_environment();
        cx.notify();
    }

    /// Relative to the collection root: the directory of the request or
    /// folder currently open, or the root itself if neither is open.
    fn default_new_request_dir(&self) -> String {
        let Ok(collection) = self.state.collection.as_ref() else {
            return String::new();
        };
        let dir = match &self.state.active_file {
            ActiveFile::Request(path) => path.parent().map(|p| p.to_path_buf()),
            ActiveFile::Folder(dir) => Some(dir.clone()),
            _ => None,
        };
        dir.and_then(|d| {
            d.strip_prefix(&collection.root)
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
    }

    fn start_new_request_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.default_new_request_dir();
        self.open_text_overlay(
            Overlay::Prompt(PromptKind::New { dir }),
            "Request name…",
            window,
            cx,
        );
    }

    fn start_rename_request_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, name)) = self
            .state
            .active_request()
            .map(|r| (r.abs_path.clone(), r.display_name.clone()))
        else {
            return;
        };
        self.open_text_overlay(
            Overlay::Prompt(PromptKind::Rename { path }),
            "Request name…",
            window,
            cx,
        );
        self.overlay_input
            .update(cx, |field, cx| field.set_text(name, cx));
    }

    fn start_duplicate_request_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, suggested)) = self
            .state
            .active_request()
            .map(|r| (r.abs_path.clone(), format!("{} copy", r.display_name)))
        else {
            return;
        };
        self.open_text_overlay(
            Overlay::Prompt(PromptKind::Duplicate { path }),
            "Request name…",
            window,
            cx,
        );
        self.overlay_input
            .update(cx, |field, cx| field.set_text(suggested, cx));
    }

    fn start_delete_request_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.state.active_request().map(|r| r.abs_path.clone()) else {
            return;
        };
        self.delete_request_with_confirm(path, window, cx);
    }

    pub(crate) fn delete_request_with_confirm(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self
            .state
            .collection
            .as_ref()
            .ok()
            .and_then(|c| c.find_request(&path))
            .map(|r| r.display_name.clone())
            .unwrap_or_else(|| path.display().to_string());
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Delete request \"{name}\"?"),
            Some("This can't be undone."),
            &["Delete", "Cancel"],
            cx,
        );
        cx.spawn(async move |weak, cx| {
            if let Ok(0) = answer.await {
                let _ = weak.update(cx, |this, cx| {
                    match epistola_engine::requests::delete_request(&path) {
                        Ok(()) => {
                            this.state.refresh_collection();
                            this.state.close_request_tab_if_open(&path);
                        }
                        Err(err) => {
                            this.state.collection_action_error = Some(err.to_string());
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn submit_prompt(&mut self, kind: PromptKind, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.overlay_input.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.state.overlay_error = Some("Name can't be empty".to_string());
            cx.notify();
            return;
        }

        let result = match &kind {
            PromptKind::New { dir } => {
                epistola_engine::requests::create_request(&self.state.cwd, dir, &name, "GET", "")
            }
            PromptKind::Rename { path } => epistola_engine::requests::rename_request(path, &name),
            PromptKind::Duplicate { path } => {
                epistola_engine::requests::duplicate_request(path, &name)
            }
        };

        match result {
            Ok(new_path) => {
                self.state.refresh_collection();
                match &kind {
                    PromptKind::New { .. } | PromptKind::Duplicate { .. } => {
                        self.state.open_request(new_path);
                    }
                    PromptKind::Rename { path } => {
                        self.state.replace_request_tab(path, new_path);
                    }
                }
                self.close_overlay(window, cx);
            }
            Err(err) => {
                self.state.overlay_error = Some(err.to_string());
                cx.notify();
            }
        }
    }

    fn dismiss_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.overlay.is_some() {
            self.close_overlay(window, cx);
        }
    }

    pub(crate) fn switch_collection_with_confirm(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.has_unsaved_changes() {
            self.state.open_collection_at(path);
            cx.notify();
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "This collection has unsaved changes in one or more tabs.",
            Some("Discard them and switch anyway?"),
            &["Discard", "Cancel"],
            cx,
        );
        cx.spawn(async move |weak, cx| {
            if let Ok(0) = answer.await {
                let _ = weak.update(cx, |this, cx| {
                    this.state.open_collection_at(path);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn close_tab_with_confirm(
        &mut self,
        file: ActiveFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.is_dirty(&file) {
            self.state.close_tab(&file);
            cx.notify();
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "This tab has unsaved changes.",
            Some("Save before closing?"),
            &["Save", "Discard", "Cancel"],
            cx,
        );
        cx.spawn(async move |weak, cx| match answer.await {
            Ok(0) => {
                let _ = weak.update(cx, |this, cx| {
                    editor_save::validate_and_save(&mut this.state, &file);
                    if !this.state.is_dirty(&file) {
                        this.state.close_tab(&file);
                    }
                    cx.notify();
                });
            }
            Ok(1) => {
                let _ = weak.update(cx, |this, cx| {
                    this.state.close_tab(&file);
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }

    fn select_environment(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.state.set_environment(name);
        self.close_overlay(window, cx);
    }

    fn overlay_item_count(&self) -> usize {
        match self.state.overlay {
            Some(Overlay::CommandPalette)
            | Some(Overlay::QuickOpen)
            | Some(Overlay::EnvironmentPicker) => self.overlay_items.len(),
            Some(Overlay::History) => self.state.history_entries.len(),
            Some(Overlay::Prompt(_)) | None => 0,
        }
    }

    fn move_overlay_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.overlay_item_count();
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

    fn confirm_overlay_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(overlay) = self.state.overlay.clone() else {
            return;
        };
        let selected = self.state.overlay_selected;
        match overlay {
            Overlay::CommandPalette | Overlay::QuickOpen | Overlay::EnvironmentPicker => {
                if let Some(item) = self.overlay_items.get(selected) {
                    let action = item.action.boxed_clone();
                    if item.closes_overlay {
                        self.close_overlay(window, cx);
                    }
                    window.dispatch_action(action, cx);
                }
            }
            Overlay::Prompt(kind) => self.submit_prompt(kind, window, cx),
            Overlay::History => {}
        }
    }

    fn handle_overlay_key_down(
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
}

fn format_lint_report(report: &epistola_engine::requests::LintReport) -> String {
    if report.issues.is_empty() {
        format!("Checked {} request(s) · 0 issues", report.checked)
    } else {
        let mut out = format!(
            "Checked {} request(s) · {} issue(s)\n",
            report.checked,
            report.issues.len()
        );
        for issue in &report.issues {
            out.push_str(&format!("  {}: {}\n", issue.path.display(), issue.message));
        }
        out
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

    if state.collection.is_ok() {
        items.push(PickerItem::new("New Request", NewRequest).keep_overlay_open());
        items.push(PickerItem::new("Lint collection", LintCollection).detail("⌘⇧L"));
        items.push(PickerItem::new("Switch environment", CycleEnvironment).detail("⌘E"));
    }

    items.push(PickerItem::new("Open settings", OpenSettings).detail("⌘,"));

    items
}

fn quick_open_items(state: &AppState) -> Vec<PickerItem> {
    let Ok(collection) = &state.collection else {
        return Vec::new();
    };
    collection
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
        .collect()
}

fn environment_picker_items(state: &AppState) -> Vec<PickerItem> {
    let Ok(collection) = &state.collection else {
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

impl Render for EpistolaGui {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        let is_fullscreen = window.is_fullscreen();

        let show_drawer = self.state.view == View::Workspace
            && !self.state.drawer_collapsed
            && (self.state.active_file.is_request()
                || !matches!(self.state.active_activity(), ActivityResult::Idle));

        let viewport: gpui::AnyElement = match self.state.view {
            View::Home => {
                home::render_home(&self.state, &self.editor_focus_handle, cx).into_any_element()
            }
            View::Workspace => {
                let sidebar_width = if self.state.sidebar_collapsed {
                    px(0.)
                } else {
                    self.state.sidebar_width
                };
                let editor_max_width =
                    window.viewport_size().width - sidebar_width - activity_rail::RAIL_WIDTH;
                let editor_max_width = if editor_max_width < px(0.) {
                    px(0.)
                } else {
                    editor_max_width
                };
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.))
                    .when(!self.state.sidebar_collapsed, |el| {
                        el.child(sidebar::render_sidebar(&self.state, cx))
                    })
                    .child(editor::render_editor(
                        &self.state,
                        self.editor_focus_handle.clone(),
                        self.editor_scroll_handle.clone(),
                        editor_max_width,
                        cx,
                    ))
                    .into_any_element()
            }
        };

        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.ink)
            .text_color(theme.text)
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.handle_overlay_key_down(event, window, cx)
            }))
            .on_mouse_move(cx.listener(EpistolaGui::on_root_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(EpistolaGui::stop_resizing))
            .on_mouse_up_out(MouseButton::Left, cx.listener(EpistolaGui::stop_resizing))
            .on_action(cx.listener(|this, _: &ToggleCommandPalette, window, cx| {
                this.toggle_command_palette(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleQuickOpen, window, cx| {
                this.toggle_quick_open(window, cx)
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, _window, cx| this.open_settings(cx)))
            .on_action(cx.listener(|this, _: &GoHome, _window, cx| this.go_home(cx)))
            .on_action(cx.listener(|this, _: &GoWorkspace, _window, cx| this.go_workspace(cx)))
            .on_action(
                cx.listener(|this, _: &RunActiveRequest, _window, cx| this.run_active_request(cx)),
            )
            .on_action(cx.listener(|this, _: &ShowResolvedRequest, _window, cx| {
                this.show_resolved_request(cx)
            }))
            .on_action(
                cx.listener(|this, _: &LintCollection, _window, cx| this.lint_collection(cx)),
            )
            .on_action(cx.listener(|this, _: &CycleEnvironment, _window, cx| {
                this.cycle_environment_action(cx)
            }))
            .on_action(cx.listener(|_this, _: &OpenCollection, _window, cx| {
                crate::actions::spawn_open_collection(cx)
            }))
            .on_action(cx.listener(|_this, _: &NewCollection, _window, cx| {
                crate::actions::spawn_new_collection(cx)
            }))
            .on_action(cx.listener(|this, _: &OpenEnvironmentPicker, window, cx| {
                this.open_environment_picker(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &OpenHistory, window, cx| this.open_history(window, cx)),
            )
            .on_action(cx.listener(|this, _: &NewRequest, window, cx| {
                this.start_new_request_prompt(window, cx)
            }))
            .on_action(cx.listener(|this, _: &RenameRequest, window, cx| {
                this.start_rename_request_prompt(window, cx)
            }))
            .on_action(cx.listener(|this, _: &DuplicateRequest, window, cx| {
                this.start_duplicate_request_prompt(window, cx)
            }))
            .on_action(cx.listener(|this, _: &DeleteRequest, window, cx| {
                this.start_delete_request_confirm(window, cx)
            }))
            .on_action(cx.listener(|this, action: &OpenRequestFile, _window, cx| {
                this.state.open_request(action.path.clone());
                cx.notify();
            }))
            .on_action(cx.listener(|this, action: &OpenFolderDoc, _window, cx| {
                this.state.open_folder_doc(action.dir.clone());
                cx.notify();
            }))
            .on_action(
                cx.listener(|this, action: &OpenEnvironmentDoc, _window, cx| {
                    this.state.open_environment_doc(action.name.clone());
                    cx.notify();
                }),
            )
            .on_action(cx.listener(|this, action: &SwitchTab, _window, cx| {
                this.state.switch_tab(action.file.clone());
                cx.notify();
            }))
            .on_action(cx.listener(|this, action: &CloseTab, window, cx| {
                this.close_tab_with_confirm(action.file.clone(), window, cx);
            }))
            .on_action(cx.listener(|this, action: &SelectEnvironment, window, cx| {
                this.select_environment(action.name.clone(), window, cx);
            }))
            .on_action(
                cx.listener(|this, action: &OpenRecentCollection, window, cx| {
                    this.switch_collection_with_confirm(action.path.clone(), window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &SelectResponseSubtab, _window, cx| {
                    this.state.response_subtab = action.subtab;
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|this, _: &Dismiss, window, cx| this.dismiss_overlay(window, cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleSidebar, _window, cx| this.toggle_sidebar(cx)))
            .on_action(cx.listener(|this, _: &ToggleDrawer, _window, cx| this.toggle_drawer(cx)))
            .on_action(
                cx.listener(|this, action: &ToggleFolderCollapse, _window, cx| {
                    this.toggle_folder_collapse(action.dir.clone(), cx);
                }),
            )
            .child(titlebar::render_titlebar(&self.state, is_fullscreen, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(activity_rail::render_activity_rail(&self.state, cx))
                    .child(div().flex().flex_1().min_w(px(0.)).child(viewport)),
            )
            .when(show_drawer, |el| {
                el.child(response_drawer::render_response_drawer(
                    self.state.active_activity(),
                    self.state.response_subtab,
                    self.state.drawer_height,
                    cx,
                ))
            })
            .child(statusbar::render_statusbar(&self.state, theme))
            .when_some(self.state.overlay.clone(), |el, overlay| match overlay {
                Overlay::CommandPalette | Overlay::QuickOpen | Overlay::EnvironmentPicker => {
                    let selected = self.state.overlay_selection(self.overlay_items.len());
                    el.child(render_picker(
                        &self.overlay_input,
                        &self.overlay_items,
                        selected,
                        &self.overlay_scroll,
                        theme,
                        cx,
                    ))
                }
                Overlay::History => {
                    let selected = self
                        .state
                        .overlay_selection(self.state.history_entries.len());
                    let on_dismiss = cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.close_overlay(window, cx)
                    });
                    el.child(history_modal::render_history_modal(
                        &self.state.history_entries,
                        selected,
                        &self.overlay_scroll,
                        theme,
                        &self.overlay_focus_handle,
                        on_dismiss,
                    ))
                }
                Overlay::Prompt(kind) => {
                    let title = match &kind {
                        PromptKind::New { .. } => "New Request",
                        PromptKind::Rename { .. } => "Rename Request",
                        PromptKind::Duplicate { .. } => "Duplicate Request",
                    };
                    let confirm_label = match &kind {
                        PromptKind::New { .. } => "Create",
                        PromptKind::Rename { .. } => "Rename",
                        PromptKind::Duplicate { .. } => "Duplicate",
                    };
                    let kind_for_confirm = kind.clone();
                    let on_confirm = cx.listener(move |this, _event: &ClickEvent, window, cx| {
                        this.submit_prompt(kind_for_confirm.clone(), window, cx);
                    });
                    let on_dismiss = cx.listener(|this, _event: &ClickEvent, window, cx| {
                        this.close_overlay(window, cx);
                    });
                    let on_cancel = cx.listener(|this, _event: &ClickEvent, window, cx| {
                        this.close_overlay(window, cx);
                    });
                    el.child(render_prompt_modal(
                        title,
                        &self.overlay_input,
                        self.state.overlay_error.as_deref(),
                        confirm_label,
                        theme,
                        on_confirm,
                        on_dismiss,
                        on_cancel,
                    ))
                }
            })
    }
}
