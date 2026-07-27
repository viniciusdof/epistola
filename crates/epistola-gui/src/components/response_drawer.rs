use epistola_core::{Header, Response};
use gpui::{div, prelude::*, px, App, ClickEvent, Context, Entity, IntoElement, Pixels, Window};

use crate::actions::SelectResponseSubtab;
use crate::buffer::ContentKind;
use crate::components::resize_handle::{resize_handle, ResizeAxis};
use crate::editor_view::EditorView;
use crate::root::EpistolaGui;
use crate::state::{ActivityResult, ResponseSubTab};
use crate::theme::Theme;

pub const DRAWER_HEIGHT: Pixels = px(168.);

/// Pretty-prints `body` as JSON or XML per the declared Content-Type;
/// returns it unchanged (as `PlainText`) if the header is absent, doesn't
/// name either format, or the body fails to parse as what it claims to be.
fn body_display(body: &str, headers: &[Header]) -> (String, ContentKind) {
    let content_type = headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))
        .map(|header| header.value.to_lowercase());

    match content_type.as_deref() {
        Some(content_type) if content_type.contains("json") => {
            match serde_json::from_str::<serde_json::Value>(body) {
                Ok(value) => (
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| body.to_string()),
                    ContentKind::Json,
                ),
                Err(_) => (body.to_string(), ContentKind::PlainText),
            }
        }
        Some(content_type) if content_type.contains("xml") => match format_xml(body) {
            Some(pretty) => (pretty, ContentKind::Xml),
            None => (body.to_string(), ContentKind::PlainText),
        },
        _ => (body.to_string(), ContentKind::PlainText),
    }
}

fn format_xml(body: &str) -> Option<String> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;
    use quick_xml::writer::Writer;

    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(true);
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(event) => writer.write_event(event).ok()?,
            Err(_) => return None,
        }
    }
    String::from_utf8(writer.into_inner()).ok()
}

fn headers_display(headers: &[Header]) -> String {
    headers
        .iter()
        .map(|header| format!("{}: {}", header.name, header.value))
        .collect::<Vec<_>>()
        .join("\n")
}

fn raw_display(response: &Response, body_text: &str) -> String {
    let mut raw = format!("HTTP/1.1 {}\n", response.status);
    for header in &response.headers {
        raw.push_str(&format!("{}: {}\n", header.name, header.value));
    }
    raw.push('\n');
    raw.push_str(body_text);
    raw
}

/// The single source of truth for "what text is currently shown" in the
/// drawer — kept independent of `Theme`/`Chip` so `EpistolaGui::sync_response_view`
/// can call it without pulling in rendering types.
pub(crate) fn compute_display(
    activity: &ActivityResult,
    subtab: ResponseSubTab,
) -> (String, ContentKind) {
    match activity {
        ActivityResult::Idle => (
            "Not run yet — open Commands (⌘K) → Run request.".to_string(),
            ContentKind::PlainText,
        ),
        ActivityResult::Running => ("Sending…".to_string(), ContentKind::PlainText),
        ActivityResult::RunSuccess(response) => {
            let body_text = response
                .body_as_str()
                .map(str::to_string)
                .unwrap_or_else(|_| format!("<{} bytes, not valid UTF-8>", response.body.len()));
            match subtab {
                ResponseSubTab::Body => body_display(&body_text, &response.headers),
                ResponseSubTab::Headers => (headers_display(&response.headers), ContentKind::PlainText),
                ResponseSubTab::Raw => (raw_display(response, &body_text), ContentKind::PlainText),
            }
        }
        ActivityResult::RunFailed(message) => (message.clone(), ContentKind::PlainText),
        ActivityResult::UnresolvedVariable { variable } => (
            format!(
                "\"{variable}\" has no value in the active environment and no request.variables default.\nAdd one, or pass an override, then run again."
            ),
            ContentKind::PlainText,
        ),
        ActivityResult::Resolved(text) => (text.clone(), ContentKind::PlainText),
        ActivityResult::ResolvedFailed(message) => (message.clone(), ContentKind::PlainText),
        ActivityResult::Linted(text) => (text.clone(), ContentKind::PlainText),
        ActivityResult::LintFailed(message) => (message.clone(), ContentKind::PlainText),
    }
}

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
    response_view: Entity<EditorView>,
    height: Pixels,
    cx: &mut Context<EpistolaGui>,
) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
    let has_headers = matches!(activity, ActivityResult::RunSuccess(_));

    let (chip, meta): (Chip, Option<String>) = match activity {
        ActivityResult::Idle => (
            Chip {
                label: "Idle".to_string(),
                color: theme.text_faint,
            },
            None,
        ),
        ActivityResult::Running => (
            Chip {
                label: "Running".to_string(),
                color: theme.accent,
            },
            None,
        ),
        ActivityResult::RunSuccess(response) => {
            let color = if response.is_success() {
                theme.success
            } else {
                theme.method_delete
            };
            (
                Chip {
                    label: response.status.to_string(),
                    color,
                },
                Some(format!(
                    "{} · {} ms · {} B",
                    response.status,
                    response.duration.as_millis(),
                    response.body.len()
                )),
            )
        }
        ActivityResult::RunFailed(_) => (
            Chip {
                label: "Failed".to_string(),
                color: theme.method_delete,
            },
            None,
        ),
        ActivityResult::UnresolvedVariable { .. } => (
            Chip {
                label: "Unresolved".to_string(),
                color: theme.method_put,
            },
            None,
        ),
        ActivityResult::Resolved(_) => (
            Chip {
                label: "Resolved".to_string(),
                color: theme.method_put,
            },
            None,
        ),
        ActivityResult::ResolvedFailed(_) => (
            Chip {
                label: "Resolve failed".to_string(),
                color: theme.method_delete,
            },
            None,
        ),
        ActivityResult::Linted(_) => (
            Chip {
                label: "Lint".to_string(),
                color: theme.method_put,
            },
            None,
        ),
        ActivityResult::LintFailed(_) => (
            Chip {
                label: "Lint failed".to_string(),
                color: theme.method_delete,
            },
            None,
        ),
    };

    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(
            resize_handle("drawer-resize-handle", ResizeAxis::Vertical, theme).on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(EpistolaGui::start_drawer_resize),
            ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .h(height)
                .overflow_hidden()
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
                .child(response_view),
        )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn headers(content_type: &str) -> Vec<Header> {
        vec![Header::new("Content-Type", content_type)]
    }

    #[test]
    fn pretty_prints_json_when_content_type_says_so() {
        assert_eq!(
            body_display(r#"{"b":2,"a":1}"#, &headers("application/json")),
            (
                "{\n  \"b\": 2,\n  \"a\": 1\n}".to_string(),
                ContentKind::Json
            )
        );
    }

    #[test]
    fn matches_content_type_case_insensitively_and_with_charset_suffix() {
        assert_eq!(
            body_display(r#"{"a":1}"#, &headers("Application/JSON; charset=utf-8")),
            ("{\n  \"a\": 1\n}".to_string(), ContentKind::Json)
        );
    }

    #[test]
    fn ignores_json_looking_body_without_a_json_content_type() {
        let body = r#"{"a":1}"#;
        assert_eq!(
            body_display(body, &[]),
            (body.to_string(), ContentKind::PlainText)
        );
        assert_eq!(
            body_display(body, &headers("text/plain")),
            (body.to_string(), ContentKind::PlainText)
        );
    }

    #[test]
    fn leaves_invalid_json_unchanged_even_with_json_content_type() {
        let body = "not json";
        assert_eq!(
            body_display(body, &headers("application/json")),
            (body.to_string(), ContentKind::PlainText)
        );
    }

    #[test]
    fn leaves_empty_body_unchanged() {
        assert_eq!(
            body_display("", &headers("application/json")),
            (String::new(), ContentKind::PlainText)
        );
    }

    #[test]
    fn pretty_prints_xml_when_content_type_says_so() {
        assert_eq!(
            body_display("<root><a>1</a><b>2</b></root>", &headers("application/xml")),
            (
                "<root>\n  <a>1</a>\n  <b>2</b>\n</root>".to_string(),
                ContentKind::Xml
            )
        );
    }

    #[test]
    fn ignores_xml_looking_body_without_an_xml_content_type() {
        let body = "<root><a>1</a></root>";
        assert_eq!(
            body_display(body, &[]),
            (body.to_string(), ContentKind::PlainText)
        );
        assert_eq!(
            body_display(body, &headers("text/plain")),
            (body.to_string(), ContentKind::PlainText)
        );
    }

    #[test]
    fn leaves_malformed_xml_unchanged_even_with_xml_content_type() {
        let body = "<root><a>unclosed</root>";
        assert_eq!(
            body_display(body, &headers("text/xml")),
            (body.to_string(), ContentKind::PlainText)
        );
    }

    #[test]
    fn headers_display_joins_name_value_lines() {
        let headers = vec![
            Header::new("Content-Type", "application/json"),
            Header::new("X-Request-Id", "abc123"),
        ];
        assert_eq!(
            headers_display(&headers),
            "Content-Type: application/json\nX-Request-Id: abc123"
        );
    }

    #[test]
    fn compute_display_is_plain_text_for_idle_and_errors() {
        assert_eq!(
            compute_display(&ActivityResult::Idle, ResponseSubTab::Body).1,
            ContentKind::PlainText
        );
        assert_eq!(
            compute_display(
                &ActivityResult::RunFailed("boom".to_string()),
                ResponseSubTab::Body
            ),
            ("boom".to_string(), ContentKind::PlainText)
        );
        assert_eq!(
            compute_display(
                &ActivityResult::Linted("ok".to_string()),
                ResponseSubTab::Body
            )
            .1,
            ContentKind::PlainText
        );
    }

    #[test]
    fn compute_display_headers_and_raw_tabs_are_plain_text() {
        use std::time::Duration;

        let response = Response {
            status: 200,
            headers: vec![Header::new("Content-Type", "application/json")],
            body: br#"{"a":1}"#.to_vec(),
            duration: Duration::from_millis(5),
        };
        let activity = ActivityResult::RunSuccess(response);

        let (headers_text, headers_kind) = compute_display(&activity, ResponseSubTab::Headers);
        assert_eq!(headers_text, "Content-Type: application/json");
        assert_eq!(headers_kind, ContentKind::PlainText);

        let (raw_text, raw_kind) = compute_display(&activity, ResponseSubTab::Raw);
        assert!(raw_text.starts_with("HTTP/1.1 200\n"));
        assert!(raw_text.contains(r#"{"a":1}"#));
        assert_eq!(raw_kind, ContentKind::PlainText);

        let (body_text, body_kind) = compute_display(&activity, ResponseSubTab::Body);
        assert_eq!(body_text, "{\n  \"a\": 1\n}");
        assert_eq!(body_kind, ContentKind::Json);
    }
}
