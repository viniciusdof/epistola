use std::path::{Path, PathBuf};

use epistola_format::{FolderManifest, RequestFile};
use gpui::{div, prelude::*, px, Context, FocusHandle, IntoElement, MouseButton, SharedString};

use crate::components::editor_text::EditorTextElement;
use crate::components::kit::{icon, IconName, MethodTag, PathClickHandler};
use crate::components::tab_strip::{self, TabStripCallbacks};
use crate::execution;
use crate::root::EpistolaGui;
use crate::state::{ActiveFile, ActivityResult, AppState};
use crate::theme::Theme;

pub struct EditorCallbacks {
    pub tab_strip: TabStripCallbacks,
    pub on_run: PathClickHandler,
}

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

fn read_toml_lines(path: &Path) -> EditorContent {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return EditorContent::Empty(format!("Could not read {}", path.display()).into());
    };
    EditorContent::Lines {
        virtual_note: None,
        lines: raw.lines().map(str::to_string).collect(),
    }
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

        ActiveFile::Folder(dir) => read_toml_lines(&dir.join("folder.toml")),
        ActiveFile::Environment(name) => match &state.collection {
            Ok(collection) => read_toml_lines(
                &collection
                    .root
                    .join("environments")
                    .join(format!("{name}.toml")),
            ),
            Err(_) => EditorContent::Empty("No collection open".into()),
        },
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

/// What the preview row shows before a run: the request's resolved (or
/// best-effort raw) URL, and whether resolving it hit an undefined
/// `{{variable}}`.
struct UrlPreview {
    text: String,
    unresolved_variable: Option<String>,
}

fn preview_url(state: &AppState, path: &Path) -> UrlPreview {
    match epistola_engine::run::resolve_saved_request(
        path,
        state.environment.as_deref(),
        Default::default(),
    ) {
        Ok((_collection, resolved)) => UrlPreview {
            text: resolved.request.url.clone(),
            unresolved_variable: None,
        },
        Err(engine_err) => {
            let raw_url = std::fs::read_to_string(path)
                .ok()
                .and_then(|raw| RequestFile::from_toml_str(&raw).ok())
                .map(|file| file.request.url)
                .unwrap_or_default();
            let unresolved_variable = match execution::classify_engine_error(engine_err) {
                ActivityResult::UnresolvedVariable { variable } => Some(variable),
                _ => None,
            };
            UrlPreview {
                text: raw_url,
                unresolved_variable,
            }
        }
    }
}

fn render_preview_row(
    state: &AppState,
    theme: Theme,
    path: &Path,
    on_run: &PathClickHandler,
) -> impl IntoElement {
    let request = state.active_request();
    let preview = preview_url(state, path);
    let running = matches!(state.active_activity(), ActivityResult::Running);
    let run_path = path.to_path_buf();
    let on_run = on_run.clone();

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
            el.child(MethodTag::new(request.method.clone(), theme))
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .text_ellipsis()
                .font_family("monospace")
                .text_color(theme.text_muted)
                .child(preview.text),
        )
        .when_some(preview.unresolved_variable, |el, variable| {
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
        })
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
                        .on_click(move |_event, window, cx| on_run(run_path.clone(), window, cx))
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
    theme: Theme,
    callbacks: EditorCallbacks,
    focus_handle: FocusHandle,
    cx: &mut Context<EpistolaGui>,
) -> impl IntoElement {
    let buffer = state.active_buffer();

    let mut body = div()
        .id("editor-code-view")
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
                .track_focus(&focus_handle)
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
        None => match load_content(state) {
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
        },
    };

    let active_request_path: Option<PathBuf> = match &state.active_file {
        ActiveFile::Request(path) => Some(path.clone()),
        _ => None,
    };
    let save_error = buffer.and_then(|buffer| buffer.save_error.clone());

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.))
        .child(tab_strip::render_tab_strip(
            state,
            theme,
            callbacks.tab_strip,
        ))
        .when_some(active_request_path, |el, path| {
            el.child(render_preview_row(state, theme, &path, &callbacks.on_run))
        })
        .when_some(save_error, |el, message| {
            el.child(render_save_error(&message, theme))
        })
        .child(body)
}
