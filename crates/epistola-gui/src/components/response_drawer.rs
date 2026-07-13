use gpui::{div, prelude::*, px, IntoElement};

use crate::state::ActivityResult;
use crate::theme::Theme;

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

pub fn render_response_drawer(theme: Theme, activity: &ActivityResult) -> impl IntoElement {
    let (chip, meta, body): (Chip, Option<String>, String) = match activity {
        ActivityResult::Idle => (
            Chip {
                label: "Idle",
                color: theme.text_faint,
            },
            None,
            "Not run yet — open Commands (⌘K) → Run request.".to_string(),
        ),
        ActivityResult::Running => (
            Chip {
                label: "Running",
                color: theme.accent,
            },
            None,
            "Sending…".to_string(),
        ),
        ActivityResult::RunSuccess {
            status,
            duration_ms,
            content_length,
            body,
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
            (
                Chip { label, color },
                Some(format!("{status} · {duration_ms} ms · {content_length} B")),
                body.clone(),
            )
        }
        ActivityResult::RunFailed(message) => (
            Chip {
                label: "Failed",
                color: theme.method_delete,
            },
            None,
            message.clone(),
        ),
        ActivityResult::Resolved(text) => (
            Chip {
                label: "Resolved",
                color: theme.method_put,
            },
            None,
            text.clone(),
        ),
        ActivityResult::ResolvedFailed(message) => (
            Chip {
                label: "Resolve failed",
                color: theme.method_delete,
            },
            None,
            message.clone(),
        ),
        ActivityResult::Linted(text) => (
            Chip {
                label: "Lint",
                color: theme.method_put,
            },
            None,
            text.clone(),
        ),
        ActivityResult::LintFailed(message) => (
            Chip {
                label: "Lint failed",
                color: theme.method_delete,
            },
            None,
            message.clone(),
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
                .child(
                    div()
                        .px(px(10.))
                        .py(px(4.))
                        .rounded(px(5.))
                        .bg(theme.surface_raised)
                        .text_color(theme.text)
                        .child("Response"),
                )
                .child(
                    div()
                        .px(px(10.))
                        .py(px(4.))
                        .rounded(px(5.))
                        .text_color(theme.text_muted)
                        .child("Headers"),
                )
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
