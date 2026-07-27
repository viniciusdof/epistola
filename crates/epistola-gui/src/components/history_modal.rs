use epistola_engine::history::HistoryEntry;
use gpui::{
    div, prelude::*, px, uniform_list, App, ClickEvent, Context, Entity, FocusHandle, IntoElement,
    SharedString, UniformListScrollHandle, Window,
};
use time::OffsetDateTime;

use crate::root::EpistolaGui;
use crate::text_field::TextField;
use crate::theme::Theme;

const ROW_HEIGHT: f32 = 46.;
const MAX_VISIBLE_ROWS: usize = 8;

/// Case-insensitive substring match against `"METHOD URL"`; keeps
/// most-recent-first order rather than ranking by relevance.
pub fn filter_history(entries: &[HistoryEntry], query: &str) -> Vec<HistoryEntry> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return entries.to_vec();
    }
    entries
        .iter()
        .filter(|entry| {
            let haystack = format!(
                "{} {}",
                entry.request.method.to_lowercase(),
                entry.request.url.to_lowercase()
            );
            haystack.contains(&query)
        })
        .cloned()
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn render_history_modal(
    query_input: &Entity<TextField>,
    entries: &[HistoryEntry],
    total_count: usize,
    selected: usize,
    scroll: &UniformListScrollHandle,
    theme: Theme,
    focus_handle: &FocusHandle,
    on_dismiss: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<EpistolaGui>,
) -> impl IntoElement {
    let empty = entries.is_empty();
    let count = entries.len();
    let rows: Vec<HistoryEntry> = entries.to_vec();
    let scroll = scroll.clone();
    let list_height = px(ROW_HEIGHT * count.clamp(1, MAX_VISIBLE_ROWS) as f32 + 12.);
    let query = query_input.read(cx).text().to_string();

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
                                .child(format!("{count} / {total_count} run(s)")),
                        ),
                )
                .child(
                    div()
                        .px(px(16.))
                        .py(px(10.))
                        .border_b_1()
                        .border_color(theme.border)
                        .font_family("monospace")
                        .text_size(px(13.))
                        .child(query_input.clone()),
                )
                .when(empty, |el| {
                    let message = if total_count == 0 {
                        "No runs recorded yet.".to_string()
                    } else {
                        format!("No matches for '{query}'")
                    };
                    el.child(
                        div()
                            .py(px(26.))
                            .text_align(gpui::TextAlign::Center)
                            .text_color(theme.text_faint)
                            .text_size(px(12.5))
                            .child(message),
                    )
                })
                .when(!empty, |el| {
                    el.child(
                        div().id("history-list").p(px(6.)).child(
                            uniform_list("history-rows", count, move |range, _window, _cx| {
                                range
                                    .map(|i| render_row(&rows[i], theme, i == selected))
                                    .collect()
                            })
                            .h(list_height)
                            .w_full()
                            .track_scroll(&scroll),
                        ),
                    )
                })
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
        .w_full()
        .h(px(ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(11.))
        .px(px(10.))
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_engine::history::{LoggedRequest, LoggedResponse};

    use super::*;

    fn entry(method: &str, url: &str) -> HistoryEntry {
        HistoryEntry {
            timestamp: OffsetDateTime::now_utc(),
            request: LoggedRequest {
                method: method.to_string(),
                url: url.to_string(),
                query: Vec::new(),
                headers: Vec::new(),
                body: None,
            },
            response: LoggedResponse {
                status: 200,
                duration_ms: 12,
                headers: Vec::new(),
                body: None,
            },
        }
    }

    #[test]
    fn empty_query_returns_all_entries_unranked() {
        let entries = vec![
            entry("GET", "https://a.example"),
            entry("POST", "https://b.example"),
        ];
        assert_eq!(filter_history(&entries, "").len(), 2);
        assert_eq!(filter_history(&entries, "   ").len(), 2);
    }

    #[test]
    fn matches_are_case_insensitive_and_preserve_order() {
        let entries = vec![
            entry("GET", "https://api.example.com/users"),
            entry("POST", "https://api.example.com/orders"),
            entry("DELETE", "https://other.example.com/users"),
        ];
        let matched = filter_history(&entries, "USERS");
        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0].request.url, "https://api.example.com/users");
        assert_eq!(matched[1].request.url, "https://other.example.com/users");
    }

    #[test]
    fn method_is_matchable_too() {
        let entries = vec![
            entry("GET", "https://a.example"),
            entry("POST", "https://a.example"),
        ];
        assert_eq!(filter_history(&entries, "post").len(), 1);
    }

    #[test]
    fn no_match_returns_empty() {
        let entries = vec![entry("GET", "https://a.example")];
        assert!(filter_history(&entries, "nope").is_empty());
    }
}
