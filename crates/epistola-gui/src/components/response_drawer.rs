use std::rc::Rc;

use gpui::{div, prelude::*, px, App, ClickEvent, IntoElement, Window};

use crate::state::{ActivityResult, ResponseSubTab};
use crate::theme::Theme;

pub type SubtabSelectHandler = Rc<dyn Fn(ResponseSubTab, &mut Window, &mut App)>;

struct Chip {
    label: &'static str,
    color: gpui::Hsla,
}

fn status_chip(chip: Chip) -> impl IntoElement {
    div()
        .font_family("monospace")
        .text_size(px(11.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(chip.color)
        .bg(chip.color.opacity(0.16))
        .rounded(px(4.))
        .px(px(7.))
        .py(px(2.))
        .child(chip.label)
}

fn drawer_subtab(
    label: &'static str,
    tab: ResponseSubTab,
    active: ResponseSubTab,
    theme: Theme,
    on_select: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(label)
        .px(px(10.))
        .py(px(4.))
        .rounded(px(5.))
        .cursor_pointer()
        .when(tab == active, |el| {
            el.bg(theme.surface_raised).text_color(theme.text)
        })
        .when(tab != active, |el| el.text_color(theme.text_muted))
        .on_click(on_select)
        .child(label)
}

pub fn render_response_drawer(
    theme: Theme,
    activity: &ActivityResult,
    subtab: ResponseSubTab,
    on_select_subtab: SubtabSelectHandler,
) -> impl IntoElement {
    let has_headers = matches!(activity, ActivityResult::RunSuccess { .. });

    let (chip, meta, body): (Chip, Option<String>, gpui::AnyElement) = match activity {
        ActivityResult::Idle => (
            Chip {
                label: "Idle",
                color: theme.text_faint,
            },
            None,
            div()
                .child("Not run yet — open Commands (⌘K) → Run request.")
                .into_any_element(),
        ),
        ActivityResult::Running => (
            Chip {
                label: "Running",
                color: theme.accent,
            },
            None,
            div().child("Sending…").into_any_element(),
        ),
        ActivityResult::RunSuccess {
            status,
            duration_ms,
            content_length,
            body,
            headers,
        } => {
            let color = if (200..300).contains(status) {
                theme.success
            } else {
                theme.method_delete
            };
            let label: &'static str = if (200..300).contains(status) {
                "200 OK"
            } else {
                "Error"
            };
            let content = match subtab {
                ResponseSubTab::Body => div().child(body.clone()).into_any_element(),
                ResponseSubTab::Headers => div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .children(headers.iter().map(|(name, value)| {
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(div().text_color(theme.method_put).child(name.clone()))
                            .child(div().text_color(theme.text).child(value.clone()))
                    }))
                    .into_any_element(),
                ResponseSubTab::Raw => {
                    let mut raw = format!("HTTP/1.1 {status}\n");
                    for (name, value) in headers {
                        raw.push_str(&format!("{name}: {value}\n"));
                    }
                    raw.push('\n');
                    raw.push_str(body);
                    div().child(raw).into_any_element()
                }
            };
            (
                Chip { label, color },
                Some(format!("{status} · {duration_ms} ms · {content_length} B")),
                content,
            )
        }
        ActivityResult::RunFailed(message) => (
            Chip {
                label: "Failed",
                color: theme.method_delete,
            },
            None,
            div().child(message.clone()).into_any_element(),
        ),
        ActivityResult::UnresolvedVariable { variable } => (
            Chip {
                label: "Unresolved",
                color: theme.method_put,
            },
            None,
            div()
                .child(format!(
                    "\"{variable}\" has no value in the active environment and no request.variables default.\nAdd one, or pass an override, then run again."
                ))
                .into_any_element(),
        ),
        ActivityResult::Resolved(text) => (
            Chip {
                label: "Resolved",
                color: theme.method_put,
            },
            None,
            div().child(text.clone()).into_any_element(),
        ),
        ActivityResult::ResolvedFailed(message) => (
            Chip {
                label: "Resolve failed",
                color: theme.method_delete,
            },
            None,
            div().child(message.clone()).into_any_element(),
        ),
        ActivityResult::Linted(text) => (
            Chip {
                label: "Lint",
                color: theme.method_put,
            },
            None,
            div().child(text.clone()).into_any_element(),
        ),
        ActivityResult::LintFailed(message) => (
            Chip {
                label: "Lint failed",
                color: theme.method_delete,
            },
            None,
            div().child(message.clone()).into_any_element(),
        ),
    };

    div()
        .flex()
        .flex_col()
        .flex_none()
        .h(px(168.))
        .border_t_1()
        .border_color(theme.border)
        .bg(theme.surface)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(12.))
                .py(px(6.))
                .border_b_1()
                .border_color(theme.border)
                .text_size(px(11.5))
                .when(has_headers, |el| {
                    let select_body = on_select_subtab.clone();
                    let select_headers = on_select_subtab.clone();
                    let select_raw = on_select_subtab.clone();
                    el.child(drawer_subtab(
                        "Body",
                        ResponseSubTab::Body,
                        subtab,
                        theme,
                        move |_event, window, cx| select_body(ResponseSubTab::Body, window, cx),
                    ))
                    .child(drawer_subtab(
                        "Headers",
                        ResponseSubTab::Headers,
                        subtab,
                        theme,
                        move |_event, window, cx| {
                            select_headers(ResponseSubTab::Headers, window, cx)
                        },
                    ))
                    .child(drawer_subtab(
                        "Raw",
                        ResponseSubTab::Raw,
                        subtab,
                        theme,
                        move |_event, window, cx| select_raw(ResponseSubTab::Raw, window, cx),
                    ))
                })
                .when(!has_headers, |el| {
                    el.child(
                        div()
                            .px(px(10.))
                            .py(px(4.))
                            .rounded(px(5.))
                            .bg(theme.surface_raised)
                            .text_color(theme.text)
                            .child("Response"),
                    )
                })
                .child(status_chip(chip))
                .when_some(meta, |el, meta| {
                    el.child(
                        div()
                            .ml(px(8.))
                            .font_family("monospace")
                            .text_size(px(11.))
                            .text_color(theme.text_faint)
                            .child(meta),
                    )
                }),
        )
        .child(
            div()
                .id("response-body")
                .flex_1()
                .overflow_y_scroll()
                .px(px(16.))
                .py(px(10.))
                .font_family("monospace")
                .text_size(px(12.5))
                .text_color(theme.text)
                .child(body),
        )
}
