use gpui::{div, prelude::*, px, App, ClickEvent, IntoElement, SharedString, Window};

use crate::components::kit::{dot_pill, TitlebarButton};
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

pub struct TitlebarCallbacks<QO, CP>
where
    QO: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    CP: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    pub on_quick_open: QO,
    pub on_command_palette: CP,
}

pub fn render_titlebar<QO, CP>(
    state: &AppState,
    theme: Theme,
    callbacks: TitlebarCallbacks<QO, CP>,
) -> impl IntoElement
where
    QO: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    CP: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .h(px(40.))
        .px(px(14.))
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
        .child(match &state.environment {
            Some(name) => dot_pill(SharedString::from(name.clone()), theme.success, theme),
            None => dot_pill(
                SharedString::from("no environment"),
                theme.text_faint,
                theme,
            ),
        })
        .child(TitlebarButton::new("Quick Open", "⌘P", theme).on_click(callbacks.on_quick_open))
        .child(TitlebarButton::new("Commands", "⌘K", theme).on_click(callbacks.on_command_palette))
}
