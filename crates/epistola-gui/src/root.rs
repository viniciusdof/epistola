//! The window's single root view. It owns `AppState` and is the only place
//! that mutates it.

use std::path::PathBuf;

use gpui::{
    div, prelude::*, px, App, ClickEvent, Context, Entity, FocusHandle, Focusable, IntoElement,
    MouseButton, Render, UniformListScrollHandle, Window,
};
use nucleo_matcher::{Config, Matcher};

use crate::actions::{
    CloseTab, CycleEnvironment, DeleteRequest, Dismiss, DuplicateRequest, GoHome, GoWorkspace,
    LintCollection, NewCollection, NewRequest, OpenCollection, OpenCollectionPicker,
    OpenEnvironmentDoc, OpenEnvironmentPicker, OpenFolderDoc, OpenHistory, OpenRecentCollection,
    OpenRequestFile, OpenSettings, RenameRequest, RunActiveRequest, SelectEnvironment,
    SelectResponseSubtab, ShowResolvedRequest, SwitchTab, ToggleCommandPalette, ToggleDrawer,
    ToggleFolderCollapse, ToggleQuickOpen, ToggleSidebar,
};
use crate::collection;
use crate::components::history_modal;
use crate::components::picker::{render_picker, PickerItem};
use crate::components::prompt_modal::render_prompt_modal;
use crate::components::{
    editor, home, resize_handle, response_drawer, sidebar, statusbar, titlebar,
};
use crate::editor_view::{EditorSaved, EditorView};
use crate::state::{self, ActiveFile, ActivityResult, AppState, Overlay, View};
use crate::text_field::TextField;
use crate::theme::Theme;
use crate::watcher::{self, FsWatcher};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResizingPanel {
    Sidebar,
    Drawer,
}

pub struct EpistolaGui {
    pub(crate) state: AppState,
    pub(crate) editor_view: Entity<EditorView>,
    pub(crate) overlay_focus_handle: FocusHandle,
    pub(crate) overlay_input: Entity<TextField>,
    pub(crate) overlay_scroll: UniformListScrollHandle,
    pub(crate) overlay_items: Vec<PickerItem>,
    pub(crate) overlay_matcher: Matcher,
    pub(crate) resizing: Option<ResizingPanel>,
    _fs_watcher: Option<FsWatcher>,
}

impl EpistolaGui {
    pub fn new(cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        let overlay_input = cx.new(|cx| TextField::new("", cx));
        cx.observe(&overlay_input, |this, _field, cx| {
            this.refresh_overlay_items(cx);
            cx.notify();
        })
        .detach();

        let editor_view = cx.new(EditorView::new);
        cx.subscribe(&editor_view, |this, _view, event: &EditorSaved, cx| {
            if let ActiveFile::Request(path) = &event.file {
                this.state.refresh_url_preview(path);
            }
            cx.notify();
        })
        .detach();

        let state = AppState::new(cwd.clone());

        let this = Self {
            state,
            editor_view,
            overlay_focus_handle: cx.focus_handle(),
            overlay_input,
            overlay_scroll: UniformListScrollHandle::default(),
            overlay_items: Vec::new(),
            overlay_matcher: Matcher::new(Config::DEFAULT),
            resizing: None,
            _fs_watcher: None,
        };
        collection::spawn_load_collection(cwd, cx);
        this
    }

    /// Re-establishes the file watch once a collection load lands on `state`.
    pub(crate) fn respawn_watcher(&mut self, cx: &mut Context<Self>) {
        self._fs_watcher = self
            .state
            .collection()
            .and_then(|collection| watcher::spawn_watch(collection.root.clone(), cx));
    }

    pub(crate) fn handle_fs_events(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let collection_root = self.state.collection().map(|c| c.root.as_path());
        let reloaded = self
            .editor_view
            .update(cx, |ev, _| ev.handle_fs_events(&paths, collection_root));
        for path in reloaded {
            self.state.refresh_url_preview(&path);
        }
        collection::spawn_refresh_collection(self.state.cwd.clone(), cx);
        cx.notify();
    }

    /// Ensures `EditorView` has a buffer for (and knows about) the currently
    /// active tab — called after any `AppState` mutation that opens, closes,
    /// switches, or renames a tab, since buffer storage lives on `EditorView`
    /// while tab/file bookkeeping stays on `AppState`.
    pub(crate) fn sync_editor_view(&mut self, cx: &mut Context<Self>) {
        let file = self.state.active_file.clone();
        if !self.editor_view.read(cx).has_buffer(&file) {
            if let Some((text, read_only)) =
                state::load_buffer_source(&file, self.state.collection())
            {
                self.editor_view
                    .update(cx, |ev, _| ev.ensure_buffer(file.clone(), text, read_only));
            }
        }
        let collection_root = self.state.collection().map(|c| c.root.clone());
        self.editor_view.update(cx, |ev, _| {
            ev.set_active_file(file.clone());
            ev.set_collection_root(collection_root);
        });
    }
}

impl Focusable for EpistolaGui {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor_view.read(cx).focus_handle.clone()
    }
}

impl EpistolaGui {
    pub(crate) fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.close_overlay();
        let focus_handle = self.editor_view.read(cx).focus_handle.clone();
        window.focus(&focus_handle, cx);
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
                let focus_handle = self.editor_view.read(cx).focus_handle.clone();
                home::render_home(&self.state, &focus_handle, cx).into_any_element()
            }
            View::Workspace => {
                let sidebar_width = if self.state.sidebar_collapsed {
                    px(0.)
                } else {
                    self.state.sidebar_width + resize_handle::HANDLE_SIZE
                };
                let editor_max_width = window.viewport_size().width - sidebar_width;
                let editor_max_width = if editor_max_width < px(0.) {
                    px(0.)
                } else {
                    editor_max_width
                };
                let snapshot = self.editor_view.read(cx).snapshot(&self.state.open_tabs);
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.))
                    .when(!self.state.sidebar_collapsed, |el| {
                        el.child(sidebar::render_sidebar(&self.state, cx))
                    })
                    .child(editor::render_editor(
                        &self.state,
                        snapshot,
                        editor_max_width,
                        self.editor_view.clone(),
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
            .on_action(cx.listener(|this, _: &OpenCollectionPicker, window, cx| {
                this.open_collection_picker(window, cx)
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
                this.sync_editor_view(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, action: &OpenFolderDoc, _window, cx| {
                this.state.open_folder_doc(action.dir.clone());
                this.sync_editor_view(cx);
                cx.notify();
            }))
            .on_action(
                cx.listener(|this, action: &OpenEnvironmentDoc, _window, cx| {
                    this.state.open_environment_doc(action.name.clone());
                    this.sync_editor_view(cx);
                    cx.notify();
                }),
            )
            .on_action(cx.listener(|this, action: &SwitchTab, _window, cx| {
                this.state.switch_tab(action.file.clone());
                this.sync_editor_view(cx);
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
                Overlay::CommandPalette
                | Overlay::QuickOpen
                | Overlay::EnvironmentPicker
                | Overlay::SwitchCollection => {
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
                    let title = kind.title();
                    let confirm_label = kind.confirm_label();
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
