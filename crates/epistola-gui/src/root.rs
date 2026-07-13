//! The window's single root view. It owns `AppState` and is the only place
//! that mutates it.

use std::path::PathBuf;
use std::rc::Rc;

use epistola_core::{Body, Request};
use gpui::{
    div, prelude::*, px, App, ClickEvent, Context, IntoElement, Render, WeakEntity, Window,
};

use crate::components::kit::MethodTag;
use crate::components::palette::{render_palette_overlay, PaletteItem, SelectHandler};
use crate::components::sidebar::SidebarCallbacks;
use crate::components::titlebar::TitlebarCallbacks;
use crate::components::{
    activity_rail, editor, home, response_drawer, sidebar, statusbar, titlebar,
};
use crate::execution;
use crate::state::{ActivityResult, AppState, Overlay, View};
use crate::theme::Theme;

pub struct EpistolaGui {
    pub(crate) state: AppState,
    theme: Theme,
}

impl EpistolaGui {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            state: AppState::new(cwd),
            theme: Theme::dark(),
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

    if let Some(path) = state.active_request().map(|r| r.abs_path.clone()) {
        items.push(
            PaletteItem::new(
                "Run request",
                make_action(weak.clone(), move |this, cx| {
                    execution::spawn_run(path.clone(), this.state.environment.clone(), cx);
                }),
            )
            .shortcut("⌘⏎"),
        );

        let resolve_path = state.active_request().map(|r| r.abs_path.clone());
        items.push(
            PaletteItem::new(
                "Show resolved request",
                make_action(weak.clone(), move |this, _cx| {
                    let Some(path) = resolve_path.clone() else {
                        return;
                    };
                    let result = epistola_engine::run::resolve_saved_request(
                        &path,
                        this.state.environment.as_deref(),
                        Default::default(),
                    )
                    .map(|(_collection, resolved)| format_resolved_request(&resolved.request))
                    .map_err(|err| err.to_string());
                    this.state.activity = match result {
                        Ok(text) => ActivityResult::Resolved(text),
                        Err(err) => ActivityResult::ResolvedFailed(err),
                    };
                }),
            )
            .shortcut("⌘⇧R"),
        );
    }

    if state.collection.is_ok() {
        items.push(
            PaletteItem::new(
                "Lint collection",
                make_action(weak.clone(), |this, _cx| {
                    let result = epistola_engine::discovery::discover_collection(&this.state.cwd)
                        .and_then(|collection| {
                            epistola_engine::requests::lint_collection(
                                &collection,
                                this.state.environment.as_deref(),
                            )
                        })
                        .map(|report| format_lint_report(&report))
                        .map_err(|err| err.to_string());
                    this.state.activity = match result {
                        Ok(text) => ActivityResult::Linted(text),
                        Err(err) => ActivityResult::LintFailed(err),
                    };
                }),
            )
            .shortcut("⌘⇧L"),
        );

        items.push(
            PaletteItem::new(
                "Switch environment",
                make_action(weak.clone(), |this, _cx| this.state.cycle_environment()),
            )
            .shortcut("⌘E"),
        );
    }

    items.push(
        PaletteItem::new(
            "Open settings",
            make_action(weak.clone(), |this, _cx| this.state.open_config()),
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let weak = cx.entity().downgrade();

        let rail_callbacks = activity_rail::RailCallbacks {
            on_home: cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.state.view = View::Home;
                cx.notify();
            }),
            on_workspace: cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.state.view = View::Workspace;
                cx.notify();
            }),
            on_settings: cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.state.open_config();
                cx.notify();
            }),
        };

        let titlebar_callbacks = TitlebarCallbacks {
            on_quick_open: cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.state.overlay = Some(Overlay::QuickOpen);
                cx.notify();
            }),
            on_command_palette: cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.state.overlay = Some(Overlay::CommandPalette);
                cx.notify();
            }),
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
        };

        let home_callbacks = home::HomeCallbacks {
            on_open_recent: {
                let weak = weak.clone();
                Rc::new(move |path: PathBuf, _window: &mut Window, cx: &mut App| {
                    let _ = weak.update(cx, |this, cx| {
                        this.state.open_collection_at(path);
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

        let show_drawer = self.state.view == View::Workspace
            && (self.state.active_file.is_request()
                || !matches!(self.state.activity, ActivityResult::Idle));

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
                .child(editor::render_editor(&self.state, theme))
                .into_any_element(),
        };

        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.ink)
            .text_color(theme.text)
            .child(titlebar::render_titlebar(
                &self.state,
                theme,
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
                    .child(div().flex_1().min_w(px(0.)).child(viewport)),
            )
            .when(show_drawer, |el| {
                el.child(response_drawer::render_response_drawer(
                    theme,
                    &self.state.activity,
                ))
            })
            .child(statusbar::render_statusbar(&self.state, theme))
            .when_some(self.state.overlay, |el, overlay| {
                let (placeholder, items): (&'static str, Vec<PaletteItem>) = match overlay {
                    Overlay::CommandPalette => (
                        "Type a command…",
                        command_palette_items(weak.clone(), &self.state),
                    ),
                    Overlay::QuickOpen => (
                        "Go to request…",
                        quick_open_items(weak.clone(), &self.state, theme),
                    ),
                };
                let on_dismiss = cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.state.overlay = None;
                    cx.notify();
                });
                el.child(render_palette_overlay(
                    placeholder,
                    items,
                    theme,
                    on_dismiss,
                ))
            })
    }
}
