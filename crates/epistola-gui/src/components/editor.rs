use std::path::{Path, PathBuf};

use gpui::{div, prelude::*, px, Context, Entity, IntoElement, Pixels};

use crate::actions::RunActiveRequest;
use crate::buffer::ContentKind;
use crate::components::kit::{dispatch_on_click, icon, IconName, MethodTag};
use crate::components::tab_strip;
use crate::editor_view::{EditorSnapshot, EditorView};
use crate::root::EpistolaGui;
use crate::state::{ActiveFile, ActivityResult, AppState};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Key,
    Punct,
    String,
    Var,
    Comment,
    Section,
    Number,
    Plain,
}

impl TokenKind {
    pub(crate) fn color(self, theme: Theme) -> gpui::Hsla {
        match self {
            TokenKind::Key => theme.text,
            TokenKind::Punct => theme.text_muted,
            TokenKind::String => theme.method_get,
            TokenKind::Var => theme.accent,
            TokenKind::Comment => theme.text_faint,
            TokenKind::Section => theme.method_put,
            TokenKind::Number => theme.method_put,
            TokenKind::Plain => theme.text,
        }
    }
}

fn find_top_level_eq(s: &str) -> Option<usize> {
    let mut in_string = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '=' if !in_string => return Some(i),
            _ => {}
        }
    }
    None
}

fn split_trailing_comment(s: &str) -> (&str, Option<&str>) {
    let mut in_string = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '#' if !in_string => return (s[..i].trim_end(), Some(&s[i..])),
            _ => {}
        }
    }
    (s, None)
}

fn tokenize_quoted_string(value: &str, spans: &mut Vec<(TokenKind, usize)>) {
    let mut rest = value;
    loop {
        let Some(start) = rest.find("{{") else {
            spans.push((TokenKind::String, rest.len()));
            return;
        };
        if start > 0 {
            spans.push((TokenKind::String, start));
        }
        match rest[start..].find("}}") {
            Some(len) => {
                let end = start + len + 2;
                spans.push((TokenKind::Var, end - start));
                rest = &rest[end..];
            }
            None => {
                spans.push((TokenKind::String, rest.len() - start));
                return;
            }
        }
    }
}

fn tokenize_value(value: &str, spans: &mut Vec<(TokenKind, usize)>) {
    if value.starts_with('"') {
        tokenize_quoted_string(value, spans);
        return;
    }
    let is_number_like = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-');
    if value == "true" || value == "false" || is_number_like {
        spans.push((TokenKind::Number, value.len()));
        return;
    }
    if !value.is_empty() {
        spans.push((TokenKind::Plain, value.len()));
    }
}

/// Returns `(TokenKind, length)` spans that partition `line` exactly (lengths sum to
/// `line.len()`), so callers can build `TextRun`s without allocating a copy of each token.
pub(crate) fn tokenize_line(line: &str) -> Vec<(TokenKind, usize)> {
    let indent_len = line.len() - line.trim_start().len();
    let mut spans = Vec::new();
    if indent_len > 0 {
        spans.push((TokenKind::Plain, indent_len));
    }
    let rest = &line[indent_len..];

    if rest.starts_with('#') {
        spans.push((TokenKind::Comment, rest.len()));
        return spans;
    }
    if rest.starts_with('[') {
        let (section, comment) = split_trailing_comment(rest);
        spans.push((TokenKind::Section, section.len()));
        if let Some(comment) = comment {
            spans.push((TokenKind::Plain, rest.len() - section.len() - comment.len()));
            spans.push((TokenKind::Comment, comment.len()));
        }
        return spans;
    }
    if let Some(eq) = find_top_level_eq(rest) {
        let key_trimmed = rest[..eq].trim_end();
        let value_part = rest[eq + 1..].trim_start();
        let value_start = rest.len() - value_part.len();
        spans.push((TokenKind::Key, key_trimmed.len()));
        spans.push((TokenKind::Punct, value_start - key_trimmed.len()));
        let (value_code, comment) = split_trailing_comment(value_part);
        tokenize_value(value_code, &mut spans);
        if let Some(comment) = comment {
            spans.push((
                TokenKind::Plain,
                value_part.len() - value_code.len() - comment.len(),
            ));
            spans.push((TokenKind::Comment, comment.len()));
        }
        return spans;
    }
    if !rest.is_empty() {
        spans.push((TokenKind::Plain, rest.len()));
    }
    spans
}

pub(crate) fn tokenize_for(kind: ContentKind, line: &str) -> Vec<(TokenKind, usize)> {
    match kind {
        ContentKind::Toml => tokenize_line(line),
        ContentKind::Json => tokenize_json_line(line),
        ContentKind::Xml => tokenize_xml_line(line),
        ContentKind::PlainText => vec![(TokenKind::Plain, line.len())],
    }
}

fn json_string_span(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 1;
    let mut escaped = false;
    while i < bytes.len() {
        let c = bytes[i];
        if escaped {
            escaped = false;
        } else if c == b'\\' {
            escaped = true;
        } else if c == b'"' {
            return i + 1;
        }
        i += 1;
    }
    s.len()
}

fn json_value_kind(value: &str) -> TokenKind {
    if value == "true" || value == "false" || value == "null" {
        return TokenKind::Number;
    }
    let is_number_like = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'));
    if is_number_like {
        TokenKind::Number
    } else {
        TokenKind::Plain
    }
}

/// Tokenizes the value half of a JSON line (after any `"key": ` prefix has
/// already been consumed) — a quoted string, a bracket opening a nested
/// container, a bare number/bool/null, or `{}`/`[]` for an empty inline
/// container — plus an optional trailing comma.
fn tokenize_json_value_tail(value: &str, spans: &mut Vec<(TokenKind, usize)>) {
    if value.is_empty() {
        return;
    }
    if value.starts_with('"') {
        let span = json_string_span(value);
        spans.push((TokenKind::String, span));
        let after = &value[span..];
        if after == "," {
            spans.push((TokenKind::Punct, 1));
        } else if !after.is_empty() {
            spans.push((TokenKind::Plain, after.len()));
        }
        return;
    }
    if value == "{" || value == "[" {
        spans.push((TokenKind::Punct, value.len()));
        return;
    }
    let (body, comma) = match value.strip_suffix(',') {
        Some(body) => (body, true),
        None => (value, false),
    };
    let kind = if body == "{}" || body == "[]" {
        TokenKind::Punct
    } else {
        json_value_kind(body)
    };
    spans.push((kind, body.len()));
    if comma {
        spans.push((TokenKind::Punct, 1));
    }
}

/// Line-based highlighter for `serde_json::to_string_pretty` output — one
/// logical token per line in the common case, not a general JSON parser.
pub(crate) fn tokenize_json_line(line: &str) -> Vec<(TokenKind, usize)> {
    let indent_len = line.len() - line.trim_start().len();
    let mut spans = Vec::new();
    if indent_len > 0 {
        spans.push((TokenKind::Plain, indent_len));
    }
    let rest = &line[indent_len..];
    if rest.is_empty() {
        return spans;
    }

    if rest.len() == 1 && matches!(rest, "{" | "}" | "[" | "]") {
        spans.push((TokenKind::Punct, 1));
        return spans;
    }
    if rest.len() == 2 && matches!(&rest[..1], "{" | "}" | "[" | "]") && &rest[1..] == "," {
        spans.push((TokenKind::Punct, 2));
        return spans;
    }

    if rest.starts_with('"') {
        let span = json_string_span(rest);
        let after = &rest[span..];
        let colon = after
            .find(':')
            .filter(|&i| after[..i].chars().all(char::is_whitespace));
        if let Some(colon) = colon {
            let trailing_spaces = after[colon + 1..].chars().take_while(|c| *c == ' ').count();
            let punct_len = colon + 1 + trailing_spaces;
            spans.push((TokenKind::Key, span));
            spans.push((TokenKind::Punct, punct_len));
            tokenize_json_value_tail(&after[punct_len..], &mut spans);
        } else {
            spans.push((TokenKind::String, span));
            if after == "," {
                spans.push((TokenKind::Punct, 1));
            } else if !after.is_empty() {
                spans.push((TokenKind::Plain, after.len()));
            }
        }
        return spans;
    }

    tokenize_json_value_tail(rest, &mut spans);
    spans
}

fn xml_string_span(s: &str) -> usize {
    let quote = s.as_bytes()[0];
    let bytes = s.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    s.len()
}

/// Line-based highlighter for pretty-printed XML — walks a line
/// left-to-right handling mixed tag/text/attribute content (quick-xml's
/// indenting writer puts `<a>1</a>`-shaped lines on one line, not one node
/// per line), not a general XML parser.
pub(crate) fn tokenize_xml_line(line: &str) -> Vec<(TokenKind, usize)> {
    let indent_len = line.len() - line.trim_start().len();
    let mut spans = Vec::new();
    if indent_len > 0 {
        spans.push((TokenKind::Plain, indent_len));
    }
    let mut rest = &line[indent_len..];
    if rest.is_empty() {
        return spans;
    }

    if rest.starts_with("<!--") {
        spans.push((TokenKind::Comment, rest.len()));
        return spans;
    }

    while !rest.is_empty() {
        let Some(tag_start) = rest.find('<') else {
            spans.push((TokenKind::Plain, rest.len()));
            break;
        };
        if tag_start > 0 {
            spans.push((TokenKind::Plain, tag_start));
            rest = &rest[tag_start..];
        }

        let closing = rest.starts_with("</");
        let punct_len = if closing { 2 } else { 1 };
        spans.push((TokenKind::Punct, punct_len));
        rest = &rest[punct_len..];

        let name_len = rest
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(rest.len());
        if name_len > 0 {
            spans.push((TokenKind::Key, name_len));
            rest = &rest[name_len..];
        }

        loop {
            let ws_len: usize = rest
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(char::len_utf8)
                .sum();
            if ws_len > 0 {
                spans.push((TokenKind::Plain, ws_len));
                rest = &rest[ws_len..];
            }
            if rest.starts_with("/>") {
                spans.push((TokenKind::Punct, 2));
                rest = &rest[2..];
                break;
            }
            if rest.starts_with('>') {
                spans.push((TokenKind::Punct, 1));
                rest = &rest[1..];
                break;
            }
            if rest.is_empty() {
                break;
            }
            let attr_name_len = rest
                .find(|c: char| c.is_whitespace() || c == '=' || c == '>' || c == '/')
                .unwrap_or(rest.len());
            if attr_name_len == 0 {
                spans.push((TokenKind::Plain, rest.len()));
                rest = "";
                break;
            }
            spans.push((TokenKind::Key, attr_name_len));
            rest = &rest[attr_name_len..];
            if rest.starts_with('=') {
                spans.push((TokenKind::Punct, 1));
                rest = &rest[1..];
                if rest.starts_with('"') || rest.starts_with('\'') {
                    let value_len = xml_string_span(rest);
                    spans.push((TokenKind::String, value_len));
                    rest = &rest[value_len..];
                }
            }
        }
    }

    spans
}

fn render_virtual_line(note: &str, theme: Theme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .px(px(16.))
        .text_color(theme.text_faint)
        .italic()
        .child(
            div()
                .flex_none()
                .w(px(24.))
                .mr(px(4.))
                .text_align(gpui::TextAlign::Right)
                .child("·"),
        )
        .child(icon(IconName::Info, px(11.), theme.text_faint))
        .child(format!("inherits Authorization from {note}"))
}

fn render_preview_row(state: &AppState, theme: Theme, path: &Path) -> impl IntoElement {
    let request = state.active_request();
    let preview = state.url_previews.get(path);
    let running = matches!(state.active_activity(), ActivityResult::Running);

    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .h(px(42.))
        .px(px(14.))
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.surface)
        .text_size(px(12.5))
        .when_some(request, |el, request| {
            el.child(MethodTag::new(request.method.clone()))
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .text_ellipsis()
                .font_family("monospace")
                .text_color(theme.text_muted)
                .child(preview.map(|p| p.text.clone()).unwrap_or_default()),
        )
        .when_some(
            preview.and_then(|p| p.unresolved_variable.clone()),
            |el, variable| {
                el.child(
                    div()
                        .flex_none()
                        .text_size(px(9.5))
                        .font_family("monospace")
                        .text_color(theme.method_put)
                        .border_1()
                        .border_color(theme.border)
                        .rounded(px(4.))
                        .px(px(6.))
                        .py(px(2.))
                        .child(format!("{{{{{variable}}}}} unresolved")),
                )
            },
        )
        .child(
            div()
                .id("editor-run-button")
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.))
                .h(px(27.))
                .px(px(12.))
                .rounded(px(6.))
                .bg(if running {
                    theme.text_faint
                } else {
                    theme.accent
                })
                .text_color(theme.accent_ink)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .when(!running, |el| {
                    el.cursor_pointer()
                        .on_click(dispatch_on_click(RunActiveRequest))
                })
                .when(running, |el| {
                    el.child(icon(IconName::Loading, px(11.), theme.accent_ink))
                })
                .child(if running { "Running" } else { "Run" }),
        )
}

fn render_save_error(message: &str, theme: Theme) -> impl IntoElement {
    div()
        .flex_none()
        .px(px(16.))
        .py(px(6.))
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.method_delete.opacity(0.12))
        .text_size(px(11.5))
        .text_color(theme.method_delete)
        .child(format!("Not saved: {message}"))
}

fn render_external_change_banner(theme: Theme) -> impl IntoElement {
    div()
        .flex_none()
        .px(px(16.))
        .py(px(6.))
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.accent.opacity(0.12))
        .text_size(px(11.5))
        .text_color(theme.accent)
        .child("Changed on disk — your unsaved edits are kept; save to overwrite.")
}

pub fn render_editor(
    state: &AppState,
    snapshot: EditorSnapshot,
    max_width: Pixels,
    editor_view: Entity<EditorView>,
    cx: &mut Context<EpistolaGui>,
) -> impl IntoElement {
    let theme = *cx.global::<Theme>();

    let active_request_path: Option<PathBuf> = match &state.active_file {
        ActiveFile::Request(path) => Some(path.clone()),
        _ => None,
    };
    let virtual_note = active_request_path
        .as_ref()
        .and_then(|path| state.url_previews.get(path))
        .and_then(|preview| preview.virtual_note.clone());

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.))
        .max_w(max_width)
        .overflow_x_hidden()
        .child(tab_strip::render_tab_strip(state, &snapshot.dirty_tabs, cx))
        .when_some(active_request_path, |el, path| {
            el.child(render_preview_row(state, theme, &path))
        })
        .when_some(virtual_note, |el, note| {
            el.child(render_virtual_line(&note, theme))
        })
        .when_some(snapshot.save_error, |el, message| {
            el.child(render_save_error(&message, theme))
        })
        .when(snapshot.external_change, |el| {
            el.child(render_external_change_banner(theme))
        })
        .child(editor_view)
}

#[cfg(test)]
mod tokenize_tests {
    use super::*;

    fn spans_cover_line(line: &str) {
        let total: usize = tokenize_line(line).iter().map(|(_, len)| len).sum();
        assert_eq!(total, line.len(), "spans for {line:?} do not cover it");
    }

    fn json_spans_cover_line(line: &str) {
        let total: usize = tokenize_json_line(line).iter().map(|(_, len)| len).sum();
        assert_eq!(total, line.len(), "json spans for {line:?} do not cover it");
    }

    fn xml_spans_cover_line(line: &str) {
        let total: usize = tokenize_xml_line(line).iter().map(|(_, len)| len).sum();
        assert_eq!(total, line.len(), "xml spans for {line:?} do not cover it");
    }

    #[test]
    fn tokenize_line_spans_are_contiguous() {
        for line in [
            "",
            "# a comment",
            "[section]",
            "[section]   # trailing comment",
            "key = value",
            "key=value",
            "key   =   value",
            "key = \"quoted {{var}} value\"",
            "key = \"unterminated {{var\"",
            "key = 42 # inline",
            "key = true",
            "    indented = 1",
            "not a key value line",
        ] {
            spans_cover_line(line);
        }
    }

    #[test]
    fn tokenize_json_line_spans_are_contiguous() {
        for line in [
            "",
            "{",
            "}",
            "},",
            "[",
            "]",
            "],",
            r#"  "key": "value","#,
            r#"  "key": "value""#,
            r#"  "escaped": "a \" quote","#,
            "  \"num\": 42,",
            "  \"num\": 42",
            "  \"flag\": true,",
            "  \"nothing\": null,",
            "  \"nested\": {",
            "  \"arr\": [",
            "  \"empty_obj\": {},",
            "  \"empty_arr\": [],",
            r#"  "a string value in an array","#,
            r#"  "last string in an array""#,
        ] {
            json_spans_cover_line(line);
        }
    }

    #[test]
    fn tokenize_json_line_colors_keys_and_values() {
        let spans = tokenize_json_line(r#"  "key": "value","#);
        assert_eq!(
            spans.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            vec![
                TokenKind::Plain,
                TokenKind::Key,
                TokenKind::Punct,
                TokenKind::String,
                TokenKind::Punct
            ]
        );
    }

    #[test]
    fn tokenize_json_line_colors_numbers_and_bools() {
        let spans = tokenize_json_line("  \"n\": 42,");
        assert_eq!(spans.last().map(|(k, _)| *k), Some(TokenKind::Punct));
        assert!(spans.contains(&(TokenKind::Number, 2)));

        let spans = tokenize_json_line("  \"flag\": true");
        assert!(spans.contains(&(TokenKind::Number, 4)));
    }

    #[test]
    fn tokenize_xml_line_spans_are_contiguous() {
        for line in [
            "",
            "<root>",
            "</root>",
            "<a>1</a>",
            "<b/>",
            r#"<tag attr="value">"#,
            r#"<tag attr="value" other='q'>"#,
            "plain text content",
            "<!-- a comment -->",
            "<a>text before<b/>text after</a>",
            "<tag/>",
            "<tag attr=\"unterminated>",
        ] {
            xml_spans_cover_line(line);
        }
    }

    #[test]
    fn tokenize_xml_line_colors_tags_and_attributes() {
        let spans = tokenize_xml_line(r#"<tag attr="value">"#);
        assert_eq!(
            spans.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            vec![
                TokenKind::Punct,
                TokenKind::Key,
                TokenKind::Plain,
                TokenKind::Key,
                TokenKind::Punct,
                TokenKind::String,
                TokenKind::Punct,
            ]
        );
    }

    #[test]
    fn tokenize_xml_line_handles_inline_text_between_tags() {
        let spans = tokenize_xml_line("<a>1</a>");
        assert_eq!(
            spans.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            vec![
                TokenKind::Punct,
                TokenKind::Key,
                TokenKind::Punct,
                TokenKind::Plain,
                TokenKind::Punct,
                TokenKind::Key,
                TokenKind::Punct,
            ]
        );
    }

    #[test]
    fn tokenize_for_dispatches_by_content_kind() {
        assert_eq!(
            tokenize_for(ContentKind::Toml, "key = 1"),
            tokenize_line("key = 1")
        );
        assert_eq!(
            tokenize_for(ContentKind::Json, "\"a\": 1"),
            tokenize_json_line("\"a\": 1")
        );
        assert_eq!(
            tokenize_for(ContentKind::Xml, "<a/>"),
            tokenize_xml_line("<a/>")
        );
        assert_eq!(
            tokenize_for(ContentKind::PlainText, "hello"),
            vec![(TokenKind::Plain, 5)]
        );
    }
}
