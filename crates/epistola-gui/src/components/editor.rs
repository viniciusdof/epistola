use std::path::{Path, PathBuf};

use gpui::{div, prelude::*, px, Context, FocusHandle, IntoElement, MouseButton, Pixels};

use crate::actions::RunActiveRequest;
use crate::components::editor_text::EditorTextElement;
use crate::components::kit::{dispatch_on_click, icon, IconName, MethodTag};
use crate::components::tab_strip;
use crate::root::EpistolaGui;
use crate::state::{ActiveFile, ActivityResult, AppState};
use crate::theme::Theme;

#[derive(Clone, Copy)]
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

pub(crate) fn tokenize_line(line: &str) -> Vec<(TokenKind, String)> {
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

pub fn render_editor(
    state: &AppState,
    focus_handle: FocusHandle,
    max_width: Pixels,
    cx: &mut Context<EpistolaGui>,
) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
    let buffer = state.active_buffer();

    let mut body = div()
        .id("editor-code-view")
        .track_focus(&focus_handle)
        .flex_1()
        .overflow_y_scroll()
        .overflow_x_scroll()
        .font_family("monospace")
        .text_size(px(13.))
        .py(px(10.));

    body = match buffer {
        Some(_) => {
            let text_element = EditorTextElement {
                gui: cx.entity(),
                theme,
                focus_handle: focus_handle.clone(),
            };
            body.key_context("Editor")
                .on_action(cx.listener(EpistolaGui::backspace))
                .on_action(cx.listener(EpistolaGui::delete))
                .on_action(cx.listener(EpistolaGui::insert_newline))
                .on_action(cx.listener(EpistolaGui::move_left))
                .on_action(cx.listener(EpistolaGui::move_right))
                .on_action(cx.listener(EpistolaGui::move_up))
                .on_action(cx.listener(EpistolaGui::move_down))
                .on_action(cx.listener(EpistolaGui::select_left))
                .on_action(cx.listener(EpistolaGui::select_right))
                .on_action(cx.listener(EpistolaGui::select_up))
                .on_action(cx.listener(EpistolaGui::select_down))
                .on_action(cx.listener(EpistolaGui::select_all))
                .on_action(cx.listener(EpistolaGui::home))
                .on_action(cx.listener(EpistolaGui::end))
                .on_action(cx.listener(EpistolaGui::paste))
                .on_action(cx.listener(EpistolaGui::cut))
                .on_action(cx.listener(EpistolaGui::copy))
                .on_action(cx.listener(EpistolaGui::save))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(EpistolaGui::on_editor_mouse_down),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(EpistolaGui::on_editor_mouse_up),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(EpistolaGui::on_editor_mouse_up),
                )
                .on_mouse_move(cx.listener(EpistolaGui::on_editor_mouse_move))
                .child(text_element)
        }
        None => body.child(
            div()
                .px(px(16.))
                .py(px(20.))
                .text_color(theme.text_faint)
                .child("No request open — press ⌘P to open one."),
        ),
    };

    let active_request_path: Option<PathBuf> = match &state.active_file {
        ActiveFile::Request(path) => Some(path.clone()),
        _ => None,
    };
    let virtual_note = active_request_path
        .as_ref()
        .and_then(|path| state.url_previews.get(path))
        .and_then(|preview| preview.virtual_note.clone());
    let save_error = buffer.and_then(|buffer| buffer.save_error.clone());

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.))
        .max_w(max_width)
        .overflow_x_hidden()
        .child(tab_strip::render_tab_strip(state, cx))
        .when_some(active_request_path, |el, path| {
            el.child(render_preview_row(state, theme, &path))
        })
        .when_some(virtual_note, |el, note| {
            el.child(render_virtual_line(&note, theme))
        })
        .when_some(save_error, |el, message| {
            el.child(render_save_error(&message, theme))
        })
        .child(body)
}
