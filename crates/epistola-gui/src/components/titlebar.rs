use gpui::{div, prelude::*, px, IntoElement, SharedString};

use crate::components::kit::{dot_pill, ClickHandler, TitlebarButton};
use crate::state::{ActiveFile, AppState, View};
use crate::theme::Theme;

fn breadcrumb(state: &AppState) -> Vec<SharedString> {
    match state.view {
        View::Home => vec!["Home".into()],
        View::Workspace => match &state.active_file {
            ActiveFile::None => match &state.collection {
                Ok(collection) => vec![collection.name.clone().into()],
                Err(_) => vec!["No collection".into()],
            },
            ActiveFile::Config => vec!["user".into(), "config.toml".into()],
            ActiveFile::Folder(dir) => {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match &state.collection {
                    Ok(collection) => vec![
                        collection.name.clone().into(),
                        name.into(),
                        "folder.toml".into(),
                    ],
                    Err(_) => vec!["…".into()],
                }
            }
            ActiveFile::Environment(name) => match &state.collection {
                Ok(collection) => vec![
                    collection.name.clone().into(),
                    "environments".into(),
                    format!("{name}.toml").into(),
                ],
                Err(_) => vec!["…".into()],
            },
            ActiveFile::Request(_) => match (&state.collection, state.active_request()) {
                (Ok(collection), Some(request)) => {
                    let mut segments = vec![collection.name.clone().into()];
                    segments.extend(
                        request
                            .rel_path
                            .parent()
                            .into_iter()
                            .flat_map(|dir| dir.components())
                            .map(|c| c.as_os_str().to_string_lossy().into_owned().into()),
                    );
                    segments.push(request.file_name.clone().into());
                    segments
                }
                _ => vec!["…".into()],
            },
        },
    }
}

fn render_breadcrumb(state: &AppState, theme: Theme) -> impl IntoElement {
    let segments = breadcrumb(state);
    let last = segments.len().saturating_sub(1);
    let mut parts: Vec<gpui::AnyElement> = Vec::with_capacity(segments.len() * 2);
    for (i, segment) in segments.into_iter().enumerate() {
        if i > 0 {
            parts.push(
                div()
                    .text_color(theme.text_faint)
                    .px(px(2.))
                    .child("›")
                    .into_any_element(),
            );
        }
        let color = if i == last {
            theme.text
        } else {
            theme.text_muted
        };
        parts.push(div().text_color(color).child(segment).into_any_element());
    }
    div().flex().items_center().children(parts)
}

pub struct TitlebarCallbacks {
    pub on_quick_open: ClickHandler,
    pub on_command_palette: ClickHandler,
    pub on_open_env_picker: ClickHandler,
}

pub fn render_titlebar(
    state: &AppState,
    theme: Theme,
    is_fullscreen: bool,
    callbacks: TitlebarCallbacks,
) -> impl IntoElement {
    let env_pill = match &state.environment {
        Some(name) => dot_pill(SharedString::from(name.clone()), theme.success, theme),
        None => dot_pill(
            SharedString::from("no environment"),
            theme.text_faint,
            theme,
        ),
    };

    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .h(px(40.))
        .pl(if is_fullscreen { px(14.) } else { px(78.) })
        .pr(px(14.))
        .border_b_1()
        .border_color(theme.border)
        .text_size(px(12.5))
        .text_color(theme.text_muted)
        .child(
            div()
                .flex_none()
                .font_family("monospace")
                .font_weight(gpui::FontWeight::BOLD)
                .text_size(px(14.))
                .text_color(theme.accent)
                .child("ϵ"),
        )
        .child(render_breadcrumb(state, theme))
        .child(div().flex_1())
        .child(
            div()
                .id("env-pill")
                .cursor_pointer()
                .on_click(callbacks.on_open_env_picker)
                .child(env_pill),
        )
        .child(TitlebarButton::new("Quick Open", "⌘P", theme).on_click(callbacks.on_quick_open))
        .child(TitlebarButton::new("Commands", "⌘K", theme).on_click(callbacks.on_command_palette))
}
