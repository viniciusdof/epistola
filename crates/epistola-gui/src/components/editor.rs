use std::path::Path;

use epistola_format::{FolderManifest, RequestFile};
use gpui::{div, prelude::*, px, IntoElement, SharedString};

use crate::components::kit::{icon, IconName, MethodTag};
use crate::state::{ActiveFile, AppState};
use crate::theme::Theme;

#[derive(Clone, Copy)]
enum TokenKind {
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
    fn color(self, theme: Theme) -> gpui::Hsla {
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

fn tokenize_quoted_string(value: &str, spans: &mut Vec<(TokenKind, String)>) {
    let mut rest = value;
    loop {
        let Some(start) = rest.find("{{") else {
            spans.push((TokenKind::String, rest.to_string()));
            return;
        };
        if start > 0 {
            spans.push((TokenKind::String, rest[..start].to_string()));
        }
        match rest[start..].find("}}") {
            Some(len) => {
                let end = start + len + 2;
                spans.push((TokenKind::Var, rest[start..end].to_string()));
                rest = &rest[end..];
            }
            None => {
                spans.push((TokenKind::String, rest[start..].to_string()));
                return;
            }
        }
    }
}

fn tokenize_value(value: &str, spans: &mut Vec<(TokenKind, String)>) {
    if value.starts_with('"') {
        tokenize_quoted_string(value, spans);
        return;
    }
    let is_number_like = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-');
    if value == "true" || value == "false" || is_number_like {
        spans.push((TokenKind::Number, value.to_string()));
        return;
    }
    if !value.is_empty() {
        spans.push((TokenKind::Plain, value.to_string()));
    }
}

fn tokenize_line(line: &str) -> Vec<(TokenKind, String)> {
    let indent_len = line.len() - line.trim_start().len();
    let mut spans = Vec::new();
    if indent_len > 0 {
        spans.push((TokenKind::Plain, line[..indent_len].to_string()));
    }
    let rest = &line[indent_len..];

    if rest.starts_with('#') {
        spans.push((TokenKind::Comment, rest.to_string()));
        return spans;
    }
    if rest.starts_with('[') {
        let (section, comment) = split_trailing_comment(rest);
        spans.push((TokenKind::Section, section.to_string()));
        if let Some(comment) = comment {
            spans.push((TokenKind::Plain, " ".to_string()));
            spans.push((TokenKind::Comment, comment.to_string()));
        }
        return spans;
    }
    if let Some(eq) = find_top_level_eq(rest) {
        let key = &rest[..eq];
        let value_part = rest[eq + 1..].trim_start();
        spans.push((TokenKind::Key, key.trim_end().to_string()));
        spans.push((TokenKind::Punct, " = ".to_string()));
        let (value_code, comment) = split_trailing_comment(value_part);
        tokenize_value(value_code, &mut spans);
        if let Some(comment) = comment {
            spans.push((TokenKind::Plain, " ".to_string()));
            spans.push((TokenKind::Comment, comment.to_string()));
        }
        return spans;
    }
    if !rest.is_empty() {
        spans.push((TokenKind::Plain, rest.to_string()));
    }
    spans
}

/// Display-only approximation — real inheritance lives in
/// `UnresolvedRequest::with_folder_inheritance`.
fn nearest_folder_auth(collection_root: &Path, request_rel_dir: &Path) -> Option<String> {
    let mut dir = request_rel_dir.to_path_buf();
    loop {
        let candidate = collection_root.join(&dir).join("folder.toml");
        if candidate.is_file() {
            if let Ok(manifest) = FolderManifest::load(&candidate) {
                if manifest.auth.is_some() {
                    return Some(if dir.as_os_str().is_empty() {
                        "folder.toml".to_string()
                    } else {
                        format!("{}/folder.toml", dir.display())
                    });
                }
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

enum EditorContent {
    Empty(SharedString),
    Lines {
        virtual_note: Option<String>,
        lines: Vec<String>,
    },
}

fn load_content(state: &AppState) -> EditorContent {
    match &state.active_file {
        ActiveFile::None => EditorContent::Empty("No request open — press ⌘P to open one.".into()),
        ActiveFile::Config => {
            let vars = epistola_format::load_global_config().unwrap_or_default();
            if vars.is_empty() {
                EditorContent::Lines {
                    virtual_note: None,
                    lines: vec!["# no global variables set yet".to_string()],
                }
            } else {
                let mut lines = vec!["[variables]".to_string()];
                lines.extend(vars.iter().map(|(k, v)| format!("{k} = \"{v}\"")));
                EditorContent::Lines {
                    virtual_note: None,
                    lines,
                }
            }
        }
        ActiveFile::Request(path) => {
            let Ok(raw) = std::fs::read_to_string(path) else {
                return EditorContent::Empty(format!("Could not read {}", path.display()).into());
            };
            let virtual_note = state.collection.as_ref().ok().and_then(|collection| {
                let rel_dir = path.strip_prefix(&collection.root).ok()?.parent()?;
                let file = RequestFile::from_toml_str(&raw).ok()?;
                if file.request.auth.is_some() {
                    return None;
                }
                nearest_folder_auth(&collection.root, rel_dir)
            });
            EditorContent::Lines {
                virtual_note,
                lines: raw.lines().map(str::to_string).collect(),
            }
        }
    }
}

fn render_line(number: usize, text: &str, theme: Theme) -> impl IntoElement {
    div()
        .flex()
        .px(px(16.))
        .child(
            div()
                .flex_none()
                .w(px(24.))
                .text_align(gpui::TextAlign::Right)
                .mr(px(8.))
                .text_color(theme.text_faint)
                .child(number.to_string()),
        )
        .child(
            div().flex().children(
                tokenize_line(text)
                    .into_iter()
                    .map(|(kind, text)| div().text_color(kind.color(theme)).child(text)),
            ),
        )
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

fn tab_label(state: &AppState, theme: Theme) -> impl IntoElement {
    let (glyph, name): (gpui::AnyElement, SharedString) = match &state.active_file {
        ActiveFile::None => (div().into_any_element(), "".into()),
        ActiveFile::Config => (
            icon(IconName::Settings, px(13.), theme.text_muted).into_any_element(),
            "config.toml".into(),
        ),
        ActiveFile::Request(path) => match state.active_request() {
            Some(request) => (
                MethodTag::new(request.method.clone(), theme).into_any_element(),
                request.file_name.clone().into(),
            ),
            None => (div().into_any_element(), path.display().to_string().into()),
        },
    };
    div()
        .flex()
        .items_center()
        .gap(px(7.))
        .px(px(14.))
        .py(px(8.))
        .border_r_1()
        .border_color(theme.border)
        .bg(theme.surface)
        .text_size(px(12.5))
        .text_color(theme.text)
        .child(glyph)
        .child(name)
}

pub fn render_editor(state: &AppState, theme: Theme) -> impl IntoElement {
    let content = load_content(state);

    let mut body = div()
        .id("editor-code-view")
        .flex_1()
        .overflow_y_scroll()
        .font_family("monospace")
        .text_size(px(13.))
        .py(px(10.));

    body = match content {
        EditorContent::Empty(message) => body.child(
            div()
                .px(px(16.))
                .py(px(20.))
                .text_color(theme.text_faint)
                .child(message),
        ),
        EditorContent::Lines {
            virtual_note,
            lines,
        } => {
            let mut rows: Vec<gpui::AnyElement> = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                rows.push(render_line(i + 1, line, theme).into_any_element());
                if i == 1 {
                    if let Some(note) = &virtual_note {
                        rows.push(render_virtual_line(note, theme).into_any_element());
                    }
                }
            }
            body.children(rows)
        }
    };

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.))
        .child(
            div()
                .flex()
                .flex_none()
                .border_b_1()
                .border_color(theme.border)
                .child(tab_label(state, theme)),
        )
        .child(body)
}
