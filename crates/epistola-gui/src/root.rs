//! The window's single root view. It owns `AppState` and is the only place
//! that mutates it.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    div, prelude::*, px, App, ClickEvent, Context, FocusHandle, Focusable, IntoElement,
    KeyDownEvent, Render, WeakEntity, Window,
};
use nucleo_matcher::{Config, Matcher};

use crate::actions::{
    CycleEnvironment, Dismiss, GoHome, LintCollection, OpenCollection, OpenSettings,
    RunActiveRequest, ShowResolvedRequest, ToggleCommandPalette, ToggleQuickOpen,
};
use crate::components::confirm_discard::{self, ClickHandler};
use crate::components::editor_text::EditorLayout;
use crate::components::env_popover;
use crate::components::history_modal;
use crate::components::palette::{
    filter_items, render_palette_overlay, PaletteItem, SelectHandler,
};
use crate::components::prompt_modal::render_prompt_modal;
use crate::components::sidebar::SidebarCallbacks;
use crate::components::tab_strip::TabStripCallbacks;
use crate::components::titlebar::TitlebarCallbacks;
use crate::components::{
    activity_rail, editor, home, response_drawer, sidebar, statusbar, titlebar,
};
use crate::editor_save;
use crate::execution;
use crate::state::{
    ActiveFile, ActivityResult, AppState, ConfirmDiscardKind, Overlay, PromptKind, ResponseSubTab,
    View,
};
use crate::theme::Theme;

pub struct EpistolaGui {
    pub(crate) state: AppState,
    theme: Theme,
    pub(crate) editor_focus_handle: FocusHandle,
    overlay_focus_handle: FocusHandle,
    pub(crate) editor_layout: Option<EditorLayout>,
    pub(crate) editor_mouse_selecting: bool,
    overlay_items: Vec<PaletteItem>,
    overlay_matcher: Matcher,
}

impl EpistolaGui {
    pub fn new(cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        Self {
            state: AppState::new(cwd),
            theme: Theme::dark(),
            editor_focus_handle: cx.focus_handle(),
            overlay_focus_handle: cx.focus_handle(),
            editor_layout: None,
            editor_mouse_selecting: false,
            overlay_items: Vec::new(),
            overlay_matcher: Matcher::new(Config::DEFAULT),
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
        window.focus(&self.overlay_focus_handle, cx);
        cx.notify();
    }

    fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.close_overlay();
        window.focus(&self.editor_focus_handle, cx);
        cx.notify();
    }

    fn toggle_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.overlay == Some(Overlay::CommandPalette) {
            self.close_overlay(window, cx);
        } else {
            self.open_overlay(Overlay::CommandPalette, window, cx);
            self.refresh_overlay_items(cx);
        }
    }

    fn toggle_quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.overlay == Some(Overlay::QuickOpen) {
            self.close_overlay(window, cx);
        } else {
            self.open_overlay(Overlay::QuickOpen, window, cx);
            self.refresh_overlay_items(cx);
        }
    }

    fn open_environment_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_overlay(Overlay::EnvironmentPicker, window, cx);
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
        let weak = cx.entity().downgrade();
        let items = match self.state.overlay {
            Some(Overlay::CommandPalette) => command_palette_items(weak, &self.state),
            Some(Overlay::QuickOpen) => quick_open_items(weak, &self.state),
            _ => Vec::new(),
        };
        self.overlay_items =
            filter_items(&mut self.overlay_matcher, items, &self.state.overlay_query);
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
        self.open_overlay(Overlay::Prompt(PromptKind::New { dir }), window, cx);
    }

    fn start_rename_request_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, name)) = self
            .state
            .active_request()
            .map(|r| (r.abs_path.clone(), r.display_name.clone()))
        else {
            return;
        };
        self.open_overlay(Overlay::Prompt(PromptKind::Rename { path }), window, cx);
        self.state.overlay_query = name;
    }

    fn start_duplicate_request_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, suggested)) = self
            .state
            .active_request()
            .map(|r| (r.abs_path.clone(), format!("{} copy", r.display_name)))
        else {
            return;
        };
        self.open_overlay(Overlay::Prompt(PromptKind::Duplicate { path }), window, cx);
        self.state.overlay_query = suggested;
    }

    fn start_delete_request_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.state.active_request().map(|r| r.abs_path.clone()) else {
            return;
        };
        self.open_overlay(Overlay::ConfirmDelete(path), window, cx);
    }

    fn submit_prompt(&mut self, kind: PromptKind, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.state.overlay_query.trim().to_string();
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

    fn confirm_delete_request(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match epistola_engine::requests::delete_request(&path) {
            Ok(()) => {
                self.state.refresh_collection();
                self.state.close_request_tab_if_open(&path);
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

    fn overlay_item_count(&self) -> usize {
        match self.state.overlay {
            Some(Overlay::CommandPalette) | Some(Overlay::QuickOpen) => self.overlay_items.len(),
            Some(Overlay::EnvironmentPicker) => self
                .state
                .collection
                .as_ref()
                .map(|collection| collection.environments.len())
                .unwrap_or(0),
            Some(Overlay::History) => self.state.history_entries.len(),
            Some(Overlay::ConfirmDiscard(_))
            | Some(Overlay::Prompt(_))
            | Some(Overlay::ConfirmDelete(_))
            | None => 0,
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
        cx.notify();
    }

    fn confirm_overlay_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(overlay) = self.state.overlay.clone() else {
            return;
        };
        let selected = self.state.overlay_selected;
        match overlay {
            Overlay::CommandPalette | Overlay::QuickOpen => {
                if let Some(item) = self.overlay_items.get(selected) {
                    let on_select = item.on_select.clone();
                    window.defer(cx, move |window, cx| on_select(window, cx));
                }
            }
            Overlay::EnvironmentPicker => {
                let name = self
                    .state
                    .collection
                    .as_ref()
                    .ok()
                    .and_then(|collection| collection.environments.get(selected).cloned());
                if let Some(name) = name {
                    self.state.set_environment(name);
                    self.close_overlay(window, cx);
                }
            }
            Overlay::Prompt(kind) => self.submit_prompt(kind, window, cx),
            Overlay::ConfirmDelete(path) => self.confirm_delete_request(path, window, cx),
            Overlay::History | Overlay::ConfirmDiscard(_) => {}
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
        let has_query = matches!(
            self.state.overlay,
            Some(Overlay::CommandPalette) | Some(Overlay::QuickOpen) | Some(Overlay::Prompt(_))
        );
        match event.keystroke.key.as_str() {
            "down" => self.move_overlay_selection(1, cx),
            "up" => self.move_overlay_selection(-1, cx),
            "enter" => self.confirm_overlay_selection(window, cx),
            "backspace" if has_query => {
                self.state.overlay_query.pop();
                self.state.overlay_selected = 0;
                self.refresh_overlay_items(cx);
                cx.notify();
            }
            _ if has_query => {
                if let Some(ch) = event.keystroke.key_char.as_deref() {
                    if !ch.is_empty() {
                        self.state.overlay_query.push_str(ch);
                        self.state.overlay_selected = 0;
                        self.refresh_overlay_items(cx);
                        cx.notify();
                    }
                }
            }
            _ => {}
        }
    }
}

fn make_action(
    weak: WeakEntity<EpistolaGui>,
    f: impl Fn(&mut EpistolaGui, &mut Context<EpistolaGui>) + 'static,
) -> SelectHandler {
    Rc::new(move |window, cx| {
        let _ = weak.update(cx, |this, cx| {
            f(this, cx);
            this.state.close_overlay();
            window.focus(&this.editor_focus_handle, cx);
            cx.notify();
        });
    })
}

/// Like `make_action`, but for palette items that hand off to another
/// overlay (a prompt or confirmation) instead of finishing the interaction —
/// it must not close that follow-up overlay the way `make_action` would.
fn make_prompt_action(
    weak: WeakEntity<EpistolaGui>,
    f: impl Fn(&mut EpistolaGui, &mut Window, &mut Context<EpistolaGui>) + 'static,
) -> SelectHandler {
    Rc::new(move |window, cx| {
        let _ = weak.update(cx, |this, cx| f(this, window, cx));
    })
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

fn command_palette_items(weak: WeakEntity<EpistolaGui>, state: &AppState) -> Vec<PaletteItem> {
    let mut items = Vec::new();

    if state.active_request().is_some() {
        items.push(
            PaletteItem::new(
                "Run request",
                make_action(weak.clone(), |this, cx| this.run_active_request(cx)),
            )
            .shortcut("⌘⏎"),
        );

        items.push(
            PaletteItem::new(
                "Show resolved request",
                make_action(weak.clone(), |this, cx| this.show_resolved_request(cx)),
            )
            .shortcut("⌘⇧R"),
        );

        items.push(PaletteItem::new(
            "Rename Request",
            make_prompt_action(weak.clone(), |this, window, cx| {
                this.start_rename_request_prompt(window, cx)
            }),
        ));

        items.push(PaletteItem::new(
            "Duplicate Request",
            make_prompt_action(weak.clone(), |this, window, cx| {
                this.start_duplicate_request_prompt(window, cx)
            }),
        ));

        items.push(PaletteItem::new(
            "Delete Request",
            make_prompt_action(weak.clone(), |this, window, cx| {
                this.start_delete_request_confirm(window, cx)
            }),
        ));
    }

    if state.collection.is_ok() {
        items.push(PaletteItem::new(
            "New Request",
            make_prompt_action(weak.clone(), |this, window, cx| {
                this.start_new_request_prompt(window, cx)
            }),
        ));

        items.push(
            PaletteItem::new(
                "Lint collection",
                make_action(weak.clone(), |this, cx| this.lint_collection(cx)),
            )
            .shortcut("⌘⇧L"),
        );

        items.push(
            PaletteItem::new(
                "Switch environment",
                make_action(weak.clone(), |this, cx| this.cycle_environment_action(cx)),
            )
            .shortcut("⌘E"),
        );
    }

    items.push(
        PaletteItem::new(
            "Open settings",
            make_action(weak.clone(), |this, cx| this.open_settings(cx)),
        )
        .shortcut("⌘,"),
    );

    items
}

fn quick_open_items(weak: WeakEntity<EpistolaGui>, state: &AppState) -> Vec<PaletteItem> {
    let Ok(collection) = &state.collection else {
        return Vec::new();
    };
    collection
        .all_requests()
        .into_iter()
        .map(|request| {
            let path = request.abs_path.clone();
            PaletteItem::new(
                request.rel_path.display().to_string(),
                make_action(weak.clone(), move |this, _cx| {
                    this.state.open_request(path.clone())
                }),
            )
            .leading_method(request.method.clone())
        })
        .collect()
}

impl Render for EpistolaGui {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let weak = cx.entity().downgrade();
        let is_fullscreen = window.is_fullscreen();

        let rail_callbacks = activity_rail::RailCallbacks {
            on_home: Box::new(cx.listener(|this, _: &ClickEvent, _window, cx| this.go_home(cx))),
            on_workspace: Box::new(
                cx.listener(|this, _: &ClickEvent, _window, cx| this.go_workspace(cx)),
            ),
            on_environments: Box::new(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_environment_picker(window, cx)
            })),
            on_history: Box::new(
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_history(window, cx)),
            ),
            on_settings: Box::new(
                cx.listener(|this, _: &ClickEvent, _window, cx| this.open_settings(cx)),
            ),
        };

        let titlebar_callbacks = TitlebarCallbacks {
            on_quick_open: Box::new(
                cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_quick_open(window, cx)),
            ),
            on_command_palette: Box::new(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.toggle_command_palette(window, cx)
            })),
            on_open_env_picker: Box::new(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_environment_picker(window, cx)
            })),
        };

        let sidebar_callbacks = SidebarCallbacks {
            on_open_request: {
                let weak = weak.clone();
                Rc::new(move |path: PathBuf, _window: &mut Window, cx: &mut App| {
                    let _ = weak.update(cx, |this, cx| {
                        this.state.open_request(path);
                        cx.notify();
                    });
                })
            },
            on_open_config: {
                let weak = weak.clone();
                Rc::new(move |_window: &mut Window, cx: &mut App| {
                    let _ = weak.update(cx, |this, cx| {
                        this.state.open_config();
                        cx.notify();
                    });
                })
            },
            on_open_folder: {
                let weak = weak.clone();
                Rc::new(move |dir: PathBuf, _window: &mut Window, cx: &mut App| {
                    let _ = weak.update(cx, |this, cx| {
                        this.state.open_folder_doc(dir);
                        cx.notify();
                    });
                })
            },
            on_open_environment: {
                let weak = weak.clone();
                Rc::new(move |name: String, _window: &mut Window, cx: &mut App| {
                    let _ = weak.update(cx, |this, cx| {
                        this.state.open_environment_doc(name);
                        cx.notify();
                    });
                })
            },
        };

        let home_callbacks = home::HomeCallbacks {
            on_open_recent: {
                let weak = weak.clone();
                Rc::new(move |path: PathBuf, _window: &mut Window, cx: &mut App| {
                    let _ = weak.update(cx, |this, cx| {
                        if this.state.has_unsaved_changes() {
                            this.state.overlay = Some(Overlay::ConfirmDiscard(
                                ConfirmDiscardKind::SwitchCollection(path),
                            ));
                        } else {
                            this.state.open_collection_at(path);
                        }
                        cx.notify();
                    });
                })
            },
            on_open_collection: cx.listener(|_this, _: &ClickEvent, _window, cx| {
                crate::actions::spawn_open_collection(cx);
            }),
            on_new_collection: cx.listener(|_this, _: &ClickEvent, _window, cx| {
                crate::actions::spawn_new_collection(cx);
            }),
        };

        let editor_callbacks = editor::EditorCallbacks {
            tab_strip: TabStripCallbacks {
                on_select: {
                    let weak = weak.clone();
                    Rc::new(
                        move |file: ActiveFile, _window: &mut Window, cx: &mut App| {
                            let _ = weak.update(cx, |this, cx| {
                                this.state.switch_tab(file);
                                cx.notify();
                            });
                        },
                    )
                },
                on_close: {
                    let weak = weak.clone();
                    Rc::new(
                        move |file: ActiveFile, _window: &mut Window, cx: &mut App| {
                            let _ = weak.update(cx, |this, cx| {
                                if this.state.is_dirty(&file) {
                                    this.state.overlay = Some(Overlay::ConfirmDiscard(
                                        ConfirmDiscardKind::CloseTab(file),
                                    ));
                                } else {
                                    this.state.close_tab(&file);
                                }
                                cx.notify();
                            });
                        },
                    )
                },
            },
            on_run: {
                let weak = weak.clone();
                Rc::new(move |path: PathBuf, _window: &mut Window, cx: &mut App| {
                    let _ = weak.update(cx, |this, cx| {
                        let environment = this.state.environment.clone();
                        execution::spawn_run(path, environment, cx);
                    });
                })
            },
        };

        let show_drawer = self.state.view == View::Workspace
            && (self.state.active_file.is_request()
                || !matches!(self.state.active_activity(), ActivityResult::Idle));

        let viewport: gpui::AnyElement = match self.state.view {
            View::Home => home::render_home(
                &self.state,
                theme,
                home_callbacks,
                &self.editor_focus_handle,
            )
            .into_any_element(),
            View::Workspace => div()
                .flex()
                .flex_1()
                .min_h(px(0.))
                .child(sidebar::render_sidebar(
                    &self.state,
                    theme,
                    sidebar_callbacks,
                ))
                .child(editor::render_editor(
                    &self.state,
                    theme,
                    editor_callbacks,
                    self.editor_focus_handle.clone(),
                    cx,
                ))
                .into_any_element(),
        };

        let on_select_subtab: response_drawer::SubtabSelectHandler = {
            let weak = weak.clone();
            Rc::new(
                move |subtab: ResponseSubTab, _window: &mut Window, cx: &mut App| {
                    let _ = weak.update(cx, |this, cx| {
                        this.state.response_subtab = subtab;
                        cx.notify();
                    });
                },
            )
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
            .on_action(cx.listener(|this, _: &ToggleCommandPalette, window, cx| {
                this.toggle_command_palette(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleQuickOpen, window, cx| {
                this.toggle_quick_open(window, cx)
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, _window, cx| this.open_settings(cx)))
            .on_action(cx.listener(|this, _: &GoHome, _window, cx| this.go_home(cx)))
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
            .on_action(
                cx.listener(|this, _: &Dismiss, window, cx| this.dismiss_overlay(window, cx)),
            )
            .child(titlebar::render_titlebar(
                &self.state,
                theme,
                is_fullscreen,
                titlebar_callbacks,
            ))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(activity_rail::render_activity_rail(
                        &self.state,
                        theme,
                        rail_callbacks,
                    ))
                    .child(div().flex().flex_1().min_w(px(0.)).child(viewport)),
            )
            .when(show_drawer, |el| {
                el.child(response_drawer::render_response_drawer(
                    theme,
                    self.state.active_activity(),
                    self.state.response_subtab,
                    on_select_subtab,
                ))
            })
            .child(statusbar::render_statusbar(&self.state, theme))
            .when_some(self.state.overlay.clone(), |el, overlay| match overlay {
                Overlay::CommandPalette => {
                    let selected = self.state.overlay_selection(self.overlay_items.len());
                    let on_dismiss = cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.close_overlay(window, cx)
                    });
                    el.child(render_palette_overlay(
                        "Type a command…",
                        &self.state.overlay_query,
                        &self.overlay_items,
                        selected,
                        theme,
                        &self.overlay_focus_handle,
                        on_dismiss,
                    ))
                }
                Overlay::QuickOpen => {
                    let selected = self.state.overlay_selection(self.overlay_items.len());
                    let on_dismiss = cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.close_overlay(window, cx)
                    });
                    el.child(render_palette_overlay(
                        "Go to request…",
                        &self.state.overlay_query,
                        &self.overlay_items,
                        selected,
                        theme,
                        &self.overlay_focus_handle,
                        on_dismiss,
                    ))
                }
                Overlay::EnvironmentPicker => {
                    let environments = self
                        .state
                        .collection
                        .as_ref()
                        .map(|c| c.environments.clone())
                        .unwrap_or_default();
                    let current = self.state.environment.clone();
                    let selected = self.state.overlay_selection(environments.len());
                    let on_select = {
                        let weak = weak.clone();
                        move |name: String, window: &mut Window, cx: &mut App| {
                            let _ = weak.update(cx, |this, cx| {
                                this.state.set_environment(name);
                                this.close_overlay(window, cx);
                            });
                        }
                    };
                    let on_dismiss = cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.close_overlay(window, cx)
                    });
                    el.child(env_popover::render_env_popover(
                        &environments,
                        current.as_deref(),
                        selected,
                        theme,
                        &self.overlay_focus_handle,
                        on_select,
                        on_dismiss,
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
                        theme,
                        &self.overlay_focus_handle,
                        on_dismiss,
                    ))
                }
                Overlay::ConfirmDiscard(kind) => {
                    let (message, save_target): (String, Option<ActiveFile>) = match &kind {
                        ConfirmDiscardKind::CloseTab(file) => (
                            "This tab has unsaved changes. Save before closing?".to_string(),
                            Some(file.clone()),
                        ),
                        ConfirmDiscardKind::SwitchCollection(_) => (
                            "This collection has unsaved changes in one or more tabs. \
                             Discard them and switch anyway?"
                                .to_string(),
                            None,
                        ),
                    };
                    let on_save: Option<ClickHandler> = save_target.map(|file| {
                        let weak = weak.clone();
                        Rc::new(
                            move |_event: &ClickEvent, _window: &mut Window, cx: &mut App| {
                                let _ = weak.update(cx, |this, cx| {
                                    editor_save::validate_and_save(&mut this.state, &file);
                                    if !this.state.is_dirty(&file) {
                                        this.state.close_tab(&file);
                                        this.state.overlay = None;
                                    }
                                    cx.notify();
                                });
                            },
                        ) as ClickHandler
                    });
                    let on_discard = {
                        let weak = weak.clone();
                        let kind = kind.clone();
                        Rc::new(
                            move |_event: &ClickEvent, _window: &mut Window, cx: &mut App| {
                                let _ = weak.update(cx, |this, cx| {
                                    match &kind {
                                        ConfirmDiscardKind::CloseTab(file) => {
                                            this.state.close_tab(file)
                                        }
                                        ConfirmDiscardKind::SwitchCollection(path) => {
                                            this.state.open_collection_at(path.clone())
                                        }
                                    }
                                    this.state.overlay = None;
                                    cx.notify();
                                });
                            },
                        )
                    };
                    let on_cancel: ClickHandler = {
                        let weak = weak.clone();
                        Rc::new(
                            move |_event: &ClickEvent, _window: &mut Window, cx: &mut App| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.state.overlay = None;
                                    cx.notify();
                                });
                            },
                        )
                    };
                    el.child(confirm_discard::render_confirm_discard(
                        &message, on_save, on_discard, on_cancel, theme,
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
                    let on_confirm: ClickHandler = {
                        let weak = weak.clone();
                        let kind = kind.clone();
                        Rc::new(
                            move |_event: &ClickEvent, window: &mut Window, cx: &mut App| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.submit_prompt(kind.clone(), window, cx);
                                });
                            },
                        )
                    };
                    let on_cancel: ClickHandler = {
                        let weak = weak.clone();
                        Rc::new(
                            move |_event: &ClickEvent, window: &mut Window, cx: &mut App| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.close_overlay(window, cx);
                                });
                            },
                        )
                    };
                    el.child(render_prompt_modal(
                        title,
                        "Request name…",
                        &self.state.overlay_query,
                        self.state.overlay_error.as_deref(),
                        confirm_label,
                        theme,
                        &self.overlay_focus_handle,
                        on_confirm,
                        on_cancel,
                    ))
                }
                Overlay::ConfirmDelete(path) => {
                    let name = self
                        .state
                        .collection
                        .as_ref()
                        .ok()
                        .and_then(|c| c.find_request(&path))
                        .map(|r| r.display_name.clone())
                        .unwrap_or_else(|| path.display().to_string());
                    let message = format!("Delete request \"{name}\"? This can't be undone.");
                    let on_confirm: ClickHandler = {
                        let weak = weak.clone();
                        let path = path.clone();
                        Rc::new(
                            move |_event: &ClickEvent, window: &mut Window, cx: &mut App| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.confirm_delete_request(path.clone(), window, cx);
                                });
                            },
                        )
                    };
                    let on_cancel: ClickHandler = {
                        let weak = weak.clone();
                        Rc::new(
                            move |_event: &ClickEvent, window: &mut Window, cx: &mut App| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.close_overlay(window, cx);
                                });
                            },
                        )
                    };
                    el.child(confirm_discard::render_confirm_delete(
                        "Delete request",
                        &message,
                        self.state.overlay_error.as_deref(),
                        on_confirm,
                        on_cancel,
                        theme,
                    ))
                }
            })
    }
}
