use epistola_engine::history::HistoryEntry;
use gpui::{div, prelude::*, px, App, ClickEvent, FocusHandle, IntoElement, SharedString, Window};
use time::OffsetDateTime;

use crate::theme::Theme;

pub fn render_history_modal(
    entries: &[HistoryEntry],
    selected: usize,
    theme: Theme,
    focus_handle: &FocusHandle,
    on_dismiss: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let empty = entries.is_empty();
    div()
        .id("history-backdrop")
        .track_focus(focus_handle)
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .items_start()
        .pt(px(78.))
        .bg(gpui::black().opacity(0.55))
        .on_click(on_dismiss)
        .child(
            div()
                .id("history-box")
                .w(px(640.))
                .max_w(px(640.))
                .bg(theme.surface_raised)
                .border_1()
                .border_color(theme.border)
                .rounded(px(10.))
                .shadow_lg()
                .on_click(|_event, _window, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .px(px(15.))
                        .py(px(13.))
                        .border_b_1()
                        .border_color(theme.border)
                        .text_size(px(13.))
                        .text_color(theme.text)
                        .child(div().flex_1().child("Request history"))
                        .child(
                            div()
                                .text_size(px(10.5))
                                .font_family("monospace")
                                .text_color(theme.text_faint)
                                .child(format!("{} run(s)", entries.len())),
                        ),
                )
                .child(
                    div()
                        .id("history-list")
                        .p(px(6.))
                        .max_h(px(380.))
                        .overflow_y_scroll()
                        .children(
                            entries
                                .iter()
                                .enumerate()
                                .map(|(i, entry)| render_row(entry, theme, i == selected)),
                        )
                        .when(empty, |el| {
                            el.child(
                                div()
                                    .py(px(26.))
                                    .text_align(gpui::TextAlign::Center)
                                    .text_color(theme.text_faint)
                                    .text_size(px(12.5))
                                    .child("No runs recorded yet."),
                            )
                        }),
                )
                .child(
                    div()
                        .px(px(14.))
                        .py(px(8.))
                        .border_t_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .text_size(px(10.5))
                        .text_color(theme.text_faint)
                        .child("Authorization and Set-Cookie header values are not redacted before a run is logged."),
                ),
        )
}

fn render_row(entry: &HistoryEntry, theme: Theme, is_selected: bool) -> impl IntoElement {
    let method = entry.request.method.as_str();
    let url = entry.request.url.as_str();
    let status = entry.response.status;
    let duration_ms = entry.response.duration_ms;
    let timestamp = entry.timestamp;

    let status_color = if (200..400).contains(&status) {
        theme.success
    } else {
        theme.method_delete
    };

    div()
        .id(SharedString::from(format!("history-row-{url}-{timestamp}")))
        .flex()
        .items_center()
        .gap(px(11.))
        .px(px(10.))
        .py(px(9.))
        .rounded(px(6.))
        .when(is_selected, |el| el.bg(theme.surface))
        .text_size(px(12.))
        .child(
            div()
                .flex_none()
                .w(px(34.))
                .font_family("monospace")
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_size(px(9.5))
                .text_color(theme.text_muted)
                .child(method.to_string()),
        )
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .text_ellipsis()
                .font_family("monospace")
                .text_size(px(11.))
                .text_color(theme.text_muted)
                .child(url.to_string()),
        )
        .child(
            div()
                .flex_none()
                .w(px(36.))
                .text_align(gpui::TextAlign::Right)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(status_color)
                .child(status.to_string()),
        )
        .child(
            div()
                .flex_none()
                .w(px(56.))
                .text_align(gpui::TextAlign::Right)
                .text_size(px(10.5))
                .text_color(theme.text_faint)
                .child(format!("{duration_ms} ms")),
        )
        .child(
            div()
                .flex_none()
                .w(px(60.))
                .text_align(gpui::TextAlign::Right)
                .text_size(px(10.5))
                .text_color(theme.text_faint)
                .child(relative_time(timestamp)),
        )
}

fn relative_time(then: OffsetDateTime) -> String {
    let elapsed = OffsetDateTime::now_utc() - then;
    let minutes = elapsed.whole_minutes();
    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else {
        format!("{}h ago", elapsed.whole_hours())
    }
}
