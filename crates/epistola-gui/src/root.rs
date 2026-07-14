//! The window's single root view. It owns `AppState` and is the only place
//! that mutates it.

use std::path::PathBuf;
use std::rc::Rc;

use epistola_core::{Body, Request};
use gpui::{
    div, prelude::*, px, App, ClickEvent, Context, FocusHandle, Focusable, IntoElement, Render,
    WeakEntity, Window,
};

use crate::actions::{
    CycleEnvironment, Dismiss, GoHome, LintCollection, OpenCollection, OpenSettings,
    RunActiveRequest, ShowResolvedRequest, ToggleCommandPalette, ToggleQuickOpen,
};
use crate::components::confirm_discard::{self, ClickHandler};
use crate::components::editor_text::EditorLayout;
use crate::components::env_popover;
use crate::components::history_modal;
use crate::components::kit::MethodTag;
use crate::components::palette::{render_palette_overlay, PaletteItem, SelectHandler};
use crate::components::sidebar::SidebarCallbacks;
use crate::components::tab_strip::TabStripCallbacks;
use crate::components::titlebar::TitlebarCallbacks;
use crate::components::{
    activity_rail, editor, home, response_drawer, sidebar, statusbar, titlebar,
};
use crate::editor_save;
use crate::execution;
use crate::state::{
    ActiveFile, ActivityResult, AppState, ConfirmDiscardKind, Overlay, ResponseSubTab, View,
};
use crate::theme::Theme;

pub struct EpistolaGui {
    pub(crate) state: AppState,
    theme: Theme,
    pub(crate) editor_focus_handle: FocusHandle,
    pub(crate) editor_layout: Option<EditorLayout>,
    pub(crate) editor_mouse_selecting: bool,
}

impl EpistolaGui {
    pub fn new(cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        Self {
            state: AppState::new(cwd),
            theme: Theme::dark(),
            editor_focus_handle: cx.focus_handle(),
            editor_layout: None,
            editor_mouse_selecting: false,
        }
    }
}

impl Focusable for EpistolaGui {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.editor_focus_handle.clone()
    }
}

impl EpistolaGui {
    fn toggle_command_palette(&mut self, cx: &mut Context<Self>) {
        self.state.overlay = if self.state.overlay == Some(Overlay::CommandPalette) {
            None
        } else {
            Some(Overlay::CommandPalette)
        };
        cx.notify();
    }

    fn toggle_quick_open(&mut self, cx: &mut Context<Self>) {
        self.state.overlay = if self.state.overlay == Some(Overlay::QuickOpen) {
            None
        } else {
            Some(Overlay::QuickOpen)
        };
        cx.notify();
    }

    fn open_environment_picker(&mut self, cx: &mut Context<Self>) {
        self.state.overlay = Some(Overlay::EnvironmentPicker);
        cx.notify();
    }

    fn open_history(&mut self, cx: &mut Context<Self>) {
        self.state.overlay = Some(Overlay::History);
        cx.notify();
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
            Ok((_collection, resolved)) => {
                ActivityResult::Resolved(format_resolved_request(&resolved.request))
            }
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

    fn dismiss_overlay(&mut self, cx: &mut Context<Self>) {
        if self.state.overlay.is_some() {
            self.state.overlay = None;
            cx.notify();
        }
    }
}

fn make_action(
    weak: WeakEntity<EpistolaGui>,
    f: impl Fn(&mut EpistolaGui, &mut Context<EpistolaGui>) + 'static,
) -> SelectHandler {
    Rc::new(move |_window, cx| {
        let _ = weak.update(cx, |this, cx| {
            f(this, cx);
            this.state.overlay = None;
            cx.notify();
        });
    })
}

fn format_resolved_request(request: &Request) -> String {
    let mut out = format!("{} {}\n", request.method.as_str(), request.url);
    if !request.query.is_empty() {
        out.push_str("\nQuery:\n");
        for (name, value) in &request.query {
            out.push_str(&format!("  {name} = {value}\n"));
        }
    }
    if !request.headers.is_empty() {
        out.push_str("\nHeaders:\n");
        for header in &request.headers {
            out.push_str(&format!("  {}: {}\n", header.name, header.value));
        }
    }
    if let Body::Bytes(bytes) = &request.body {
        out.push_str("\nBody:\n");
        out.push_str(&String::from_utf8_lossy(bytes));
        out.push('\n');
    }
    out
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
    }

    if state.collection.is_ok() {
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

fn quick_open_items(
    weak: WeakEntity<EpistolaGui>,
    state: &AppState,
    theme: Theme,
) -> Vec<PaletteItem> {
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
            .leading(MethodTag::new(request.method.clone(), theme))
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
            on_environments: Box::new(
                cx.listener(|this, _: &ClickEvent, _window, cx| this.open_environment_picker(cx)),
            ),
            on_history: Box::new(
                cx.listener(|this, _: &ClickEvent, _window, cx| this.open_history(cx)),
            ),
            on_settings: Box::new(
                cx.listener(|this, _: &ClickEvent, _window, cx| this.open_settings(cx)),
            ),
        };

        let titlebar_callbacks = TitlebarCallbacks {
            on_quick_open: Box::new(
                cx.listener(|this, _: &ClickEvent, _window, cx| this.toggle_quick_open(cx)),
            ),
            on_command_palette: Box::new(
                cx.listener(|this, _: &ClickEvent, _window, cx| this.toggle_command_palette(cx)),
            ),
            on_open_env_picker: Box::new(
                cx.listener(|this, _: &ClickEvent, _window, cx| this.open_environment_picker(cx)),
            ),
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
            View::Home => home::render_home(&self.state, theme, home_callbacks).into_any_element(),
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
            .on_action(cx.listener(|this, _: &ToggleCommandPalette, _window, cx| {
                this.toggle_command_palette(cx)
            }))
            .on_action(
                cx.listener(|this, _: &ToggleQuickOpen, _window, cx| this.toggle_quick_open(cx)),
            )
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
            .on_action(cx.listener(|this, _: &Dismiss, _window, cx| this.dismiss_overlay(cx)))
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
                    let items = command_palette_items(weak.clone(), &self.state);
                    let on_dismiss = cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.state.overlay = None;
                        cx.notify();
                    });
                    el.child(render_palette_overlay(
                        "Type a command…",
                        items,
                        theme,
                        on_dismiss,
                    ))
                }
                Overlay::QuickOpen => {
                    let items = quick_open_items(weak.clone(), &self.state, theme);
                    let on_dismiss = cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.state.overlay = None;
                        cx.notify();
                    });
                    el.child(render_palette_overlay(
                        "Go to request…",
                        items,
                        theme,
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
                    let on_select = {
                        let weak = weak.clone();
                        move |name: String, _window: &mut Window, cx: &mut App| {
                            let _ = weak.update(cx, |this, cx| {
                                this.state.set_environment(name);
                                this.state.overlay = None;
                                cx.notify();
                            });
                        }
                    };
                    let on_dismiss = cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.state.overlay = None;
                        cx.notify();
                    });
                    el.child(env_popover::render_env_popover(
                        &environments,
                        current.as_deref(),
                        theme,
                        on_select,
                        on_dismiss,
                    ))
                }
                Overlay::History => {
                    let entries = self
                        .state
                        .collection
                        .as_ref()
                        .ok()
                        .map(|collection| {
                            epistola_engine::history::read_entries(&collection.root)
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    let on_dismiss = cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.state.overlay = None;
                        cx.notify();
                    });
                    el.child(history_modal::render_history_modal(
                        &entries, theme, on_dismiss,
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
            })
    }
}
