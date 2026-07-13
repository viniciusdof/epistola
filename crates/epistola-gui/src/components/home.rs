use std::path::PathBuf;

use gpui::{div, prelude::*, px, App, ClickEvent, IntoElement, SharedString, Window};

use crate::components::kit::{ClickHandler, KbdChip, PathClickHandler};
use crate::state::AppState;
use crate::theme::Theme;

pub struct HomeCallbacks<O, N>
where
    O: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    N: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    pub on_open_recent: PathClickHandler,
    pub on_open_collection: O,
    pub on_new_collection: N,
}

fn recent_item(
    path: PathBuf,
    is_current: bool,
    theme: Theme,
    on_click: PathClickHandler,
) -> impl IntoElement {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let display_path = path.display().to_string();
    div()
        .id(SharedString::from(format!("recent-{display_path}")))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .px(px(12.))
        .py(px(10.))
        .mb(px(8.))
        .rounded(px(8.))
        .border_1()
        .cursor_pointer()
        .when(is_current, |el| el.border_color(theme.accent))
        .when(!is_current, |el| {
            el.border_color(theme.border)
                .hover(|el| el.border_color(theme.accent))
        })
        .on_click(move |_event, window, cx| on_click(path.clone(), window, cx))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .child(div().text_size(px(13.5)).text_color(theme.text).child(name))
                .child(
                    div()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .font_family("monospace")
                        .text_size(px(11.))
                        .text_color(theme.text_faint)
                        .child(display_path),
                ),
        )
        .when(is_current, |el| {
            el.child(
                div()
                    .flex_none()
                    .text_size(px(11.))
                    .text_color(theme.accent)
                    .child("current"),
            )
        })
}

fn start_action(
    label: &'static str,
    shortcut: Option<&'static str>,
    theme: Theme,
    on_click: Option<ClickHandler>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("start-action-{label}")))
        .flex()
        .items_center()
        .justify_between()
        .px(px(12.))
        .py(px(9.))
        .mb(px(8.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .text_size(px(12.5))
        .text_color(theme.text)
        .cursor_pointer()
        .hover(|el| el.border_color(theme.accent).text_color(theme.accent))
        .when_some(on_click, |el, handler| el.on_click(handler))
        .child(label)
        .when_some(shortcut, |el, shortcut| {
            el.child(KbdChip::new(shortcut, theme))
        })
}

fn shortcut_row(label: &'static str, chip: &'static str, theme: Theme) -> impl IntoElement {
    div()
        .flex()
        .justify_between()
        .py(px(3.))
        .text_size(px(12.5))
        .text_color(theme.text_muted)
        .child(label)
        .child(KbdChip::new(chip, theme))
}

pub fn render_home<O, N>(
    state: &AppState,
    theme: Theme,
    callbacks: HomeCallbacks<O, N>,
) -> impl IntoElement
where
    O: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    N: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let current_root = state.collection.as_ref().ok().map(|c| c.root.clone());

    let recent_list: gpui::AnyElement = if state.recent_collections.is_empty() {
        div()
            .px(px(12.))
            .py(px(10.))
            .text_size(px(12.5))
            .text_color(theme.text_faint)
            .child("No collections opened yet.")
            .into_any_element()
    } else {
        div()
            .children(state.recent_collections.iter().cloned().map(|path| {
                let is_current = current_root.as_deref() == Some(path.as_path());
                recent_item(path, is_current, theme, callbacks.on_open_recent.clone())
            }))
            .into_any_element()
    };

    div().id("home-pane").flex_1().overflow_y_scroll().child(
        div()
            .max_w(px(760.))
            .mx_auto()
            .px(px(24.))
            .pt(px(48.))
            .pb(px(40.))
            .child(div().text_size(px(21.)).font_weight(gpui::FontWeight::SEMIBOLD).text_color(theme.text).child("Welcome to ϵpistola"))
            .child(
                div()
                    .mb(px(32.))
                    .text_size(px(13.))
                    .text_color(theme.text_muted)
                    .child("Keyboard-first HTTP client. Edit .req.toml directly, run it, never leave the keyboard."),
            )
            .child(
                div()
                    .flex()
                    .gap(px(36.))
                    .mb(px(34.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(div().text_size(px(10.5)).text_color(theme.text_faint).mb(px(12.)).child("RECENT COLLECTIONS"))
                            .when_some(state.collection.as_ref().err(), |el, message| {
                                el.child(
                                    div()
                                        .mb(px(10.))
                                        .text_size(px(11.5))
                                        .text_color(theme.text_faint)
                                        .child(format!("No collection open right now: {message}")),
                                )
                            })
                            .child(recent_list),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(260.))
                            .child(div().text_size(px(10.5)).text_color(theme.text_faint).mb(px(12.)).child("START"))
                            .child(start_action("Open collection…", Some("⌘O"), theme, Some(Box::new(callbacks.on_open_collection))))
                            .child(start_action("New collection", None, theme, Some(Box::new(callbacks.on_new_collection))))
                            .child(start_action("Clone repository…", None, theme, None))
                            .when_some(state.collection_action_error.clone(), |el, message| {
                                el.child(div().mt(px(4.)).text_size(px(11.5)).text_color(theme.method_delete).child(message))
                            }),
                    ),
            )
            .child(
                div()
                    .pt(px(20.))
                    .border_t_1()
                    .border_color(theme.border)
                    .child(div().mb(px(14.)).text_size(px(10.5)).text_color(theme.text_faint).child("KEYBOARD SHORTCUTS"))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.))
                            .child(div().w(px(340.)).child(shortcut_row("Command palette", "⌘K", theme)))
                            .child(div().w(px(340.)).child(shortcut_row("Quick open (go to request)", "⌘P", theme)))
                            .child(div().w(px(340.)).child(shortcut_row("Settings", "⌘,", theme)))
                            .child(div().w(px(340.)).child(shortcut_row("New request", "⌘N", theme)))
                            .child(div().w(px(340.)).child(shortcut_row("Run request", "⌘⏎", theme)))
                            .child(div().w(px(340.)).child(shortcut_row("Home", "⌘0", theme))),
                    ),
            ),
    )
}
