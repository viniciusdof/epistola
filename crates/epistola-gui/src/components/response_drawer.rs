use gpui::{div, prelude::*, px, App, ClickEvent, Context, IntoElement, Window};

use crate::actions::SelectResponseSubtab;
use crate::root::EpistolaGui;
use crate::state::{ActivityResult, ResponseSubTab};
use crate::theme::Theme;

struct Chip {
    label: String,
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
        .on_click(
            move |_event: &ClickEvent, window: &mut Window, cx: &mut App| {
                window.dispatch_action(Box::new(SelectResponseSubtab { subtab: tab }), cx);
            },
        )
        .child(label)
}

pub fn render_response_drawer(
    activity: &ActivityResult,
    subtab: ResponseSubTab,
    cx: &mut Context<EpistolaGui>,
) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
    let has_headers = matches!(activity, ActivityResult::RunSuccess(_));

    let (chip, meta, body): (Chip, Option<String>, gpui::AnyElement) = match activity {
        ActivityResult::Idle => (
            Chip {
                label: "Idle".to_string(),
                color: theme.text_faint,
            },
            None,
            div()
                .child("Not run yet — open Commands (⌘K) → Run request.")
                .into_any_element(),
        ),
        ActivityResult::Running => (
            Chip {
                label: "Running".to_string(),
                color: theme.accent,
            },
            None,
            div().child("Sending…").into_any_element(),
        ),
        ActivityResult::RunSuccess(response) => {
            let color = if response.is_success() {
                theme.success
            } else {
                theme.method_delete
            };
            let status = response.status;
            let body_text = response
                .body_as_str()
                .map(str::to_string)
                .unwrap_or_else(|_| format!("<{} bytes, not valid UTF-8>", response.body.len()));
            let content = match subtab {
                ResponseSubTab::Body => div().child(body_text.clone()).into_any_element(),
                ResponseSubTab::Headers => div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .children(response.headers.iter().map(|header| {
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_color(theme.method_put)
                                    .child(header.name.clone()),
                            )
                            .child(div().text_color(theme.text).child(header.value.clone()))
                    }))
                    .into_any_element(),
                ResponseSubTab::Raw => {
                    let mut raw = format!("HTTP/1.1 {status}\n");
                    for header in &response.headers {
                        raw.push_str(&format!("{}: {}\n", header.name, header.value));
                    }
                    raw.push('\n');
                    raw.push_str(&body_text);
                    div().child(raw).into_any_element()
                }
            };
            (
                Chip {
                    label: status.to_string(),
                    color,
                },
                Some(format!(
                    "{status} · {} ms · {} B",
                    response.duration.as_millis(),
                    response.body.len()
                )),
                content,
            )
        }
        ActivityResult::RunFailed(message) => (
            Chip {
                label: "Failed".to_string(),
                color: theme.method_delete,
            },
            None,
            div().child(message.clone()).into_any_element(),
        ),
        ActivityResult::UnresolvedVariable { variable } => (
            Chip {
                label: "Unresolved".to_string(),
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
                label: "Resolved".to_string(),
                color: theme.method_put,
            },
            None,
            div().child(text.clone()).into_any_element(),
        ),
        ActivityResult::ResolvedFailed(message) => (
            Chip {
                label: "Resolve failed".to_string(),
                color: theme.method_delete,
            },
            None,
            div().child(message.clone()).into_any_element(),
        ),
        ActivityResult::Linted(text) => (
            Chip {
                label: "Lint".to_string(),
                color: theme.method_put,
            },
            None,
            div().child(text.clone()).into_any_element(),
        ),
        ActivityResult::LintFailed(message) => (
            Chip {
                label: "Lint failed".to_string(),
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
                    el.child(drawer_subtab("Body", ResponseSubTab::Body, subtab, theme))
                        .child(drawer_subtab(
                            "Headers",
                            ResponseSubTab::Headers,
                            subtab,
                            theme,
                        ))
                        .child(drawer_subtab("Raw", ResponseSubTab::Raw, subtab, theme))
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
