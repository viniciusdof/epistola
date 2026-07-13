use std::path::Path;

use gpui::{div, prelude::*, px, IntoElement, SharedString};

use crate::collection::{FolderEntry, RequestEntry};
use crate::components::kit::{icon, IconName, MethodTag, PathClickHandler};
use crate::components::palette::SelectHandler;
use crate::state::{ActiveFile, AppState};
use crate::theme::Theme;

pub type OpenRequestHandler = PathClickHandler;

fn section_label(label: impl Into<SharedString>, theme: Theme) -> impl IntoElement {
    div()
        .px(px(12.))
        .pt(px(4.))
        .text_size(px(10.5))
        .text_color(theme.text_faint)
        .child(label.into())
}

fn selectable_row(active: bool, indent: f32, theme: Theme) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .pl(px(26. + indent))
        .pr(px(10.))
        .py(px(4.))
        .cursor_pointer()
        .border_l_2()
        .when(active, |el| {
            el.border_color(theme.accent)
                .bg(theme.surface_raised)
                .text_color(theme.text)
        })
        .when(!active, |el| {
            el.border_color(gpui::transparent_black())
                .text_color(theme.text_muted)
                .hover(|el| el.bg(theme.surface))
        })
}

pub struct SidebarCallbacks {
    pub on_open_request: OpenRequestHandler,
    pub on_open_config: SelectHandler,
}

fn render_request_row(
    request: &RequestEntry,
    active: bool,
    depth: usize,
    theme: Theme,
    on_open_request: &OpenRequestHandler,
) -> impl IntoElement {
    let path = request.abs_path.clone();
    let on_open_request = on_open_request.clone();
    selectable_row(active, depth as f32 * 14., theme)
        .id(SharedString::from(format!(
            "sidebar-request-{}",
            request.rel_path.display()
        )))
        .on_click(move |_event, window, cx| on_open_request(path.clone(), window, cx))
        .aria_label(SharedString::from(request.display_name.clone()))
        .child(MethodTag::new(request.method.clone(), theme))
        .child(
            div()
                .overflow_hidden()
                .text_ellipsis()
                .child(request.file_name.clone()),
        )
}

fn render_folder_rows(
    folder: &FolderEntry,
    depth: usize,
    active_path: Option<&Path>,
    theme: Theme,
    on_open_request: &OpenRequestHandler,
    out: &mut Vec<gpui::AnyElement>,
) {
    out.push(
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(12. + depth as f32 * 14.))
            .pr(px(10.))
            .py(px(4.))
            .text_color(theme.text)
            .child(icon(IconName::ChevronDown, px(10.), theme.text_faint))
            .child(div().child(folder.name.clone()))
            .child(div().flex_1())
            .when(folder.has_folder_toml, |el| {
                el.child(
                    div()
                        .text_size(px(9.5))
                        .text_color(theme.text_faint)
                        .border_1()
                        .border_color(theme.border)
                        .rounded(px(3.))
                        .px(px(4.))
                        .child("ƒ"),
                )
            })
            .into_any_element(),
    );

    for request in &folder.requests {
        let active = active_path == Some(request.abs_path.as_path());
        out.push(
            render_request_row(request, active, depth + 1, theme, on_open_request)
                .into_any_element(),
        );
    }
    for child in &folder.folders {
        render_folder_rows(child, depth + 1, active_path, theme, on_open_request, out);
    }
}

fn render_collection_error(message: &str, theme: Theme) -> impl IntoElement {
    div()
        .px(px(12.))
        .py(px(10.))
        .text_size(px(11.5))
        .text_color(theme.text_faint)
        .child(format!("No collection here: {message}"))
}

pub fn render_sidebar(
    state: &AppState,
    theme: Theme,
    callbacks: SidebarCallbacks,
) -> impl IntoElement {
    let mut list = div()
        .id("sidebar")
        .flex()
        .flex_col()
        .flex_none()
        .w(px(216.))
        .py(px(10.))
        .border_r_1()
        .border_color(theme.border)
        .text_size(px(12.5));

    let active_path = match &state.active_file {
        ActiveFile::Request(path) => Some(path.as_path()),
        _ => None,
    };

    match &state.collection {
        Ok(collection) => {
            list = list.child(section_label(collection.name.clone(), theme));

            let mut rows: Vec<gpui::AnyElement> = Vec::new();
            for request in &collection.requests {
                let active = active_path == Some(request.abs_path.as_path());
                rows.push(
                    render_request_row(request, active, 1, theme, &callbacks.on_open_request)
                        .into_any_element(),
                );
            }
            for folder in &collection.folders {
                render_folder_rows(
                    folder,
                    0,
                    active_path,
                    theme,
                    &callbacks.on_open_request,
                    &mut rows,
                );
            }
            list = list.children(rows);

            list = list.child(section_label("environments", theme));
            if collection.environments.is_empty() {
                list = list.child(
                    div()
                        .pl(px(26.))
                        .pr(px(10.))
                        .py(px(4.))
                        .text_color(theme.text_faint)
                        .child("none yet"),
                );
            } else {
                for env in &collection.environments {
                    list = list.child(
                        div()
                            .pl(px(26.))
                            .pr(px(10.))
                            .py(px(4.))
                            .text_color(theme.text_muted)
                            .child(env.clone()),
                    );
                }
            }
        }
        Err(message) => {
            list = list.child(section_label("collection", theme));
            list = list.child(render_collection_error(message, theme));
        }
    }

    list = list.child(section_label("user", theme));
    let config_active = state.active_file == ActiveFile::Config;
    let on_open_config = callbacks.on_open_config.clone();
    list = list.child(
        selectable_row(config_active, 0., theme)
            .id("sidebar-config")
            .on_click(move |_event, window, cx| on_open_config(window, cx))
            .child(div().flex_none().w(px(34.)).child(icon(
                IconName::Settings,
                px(12.),
                theme.text_muted,
            )))
            .child(div().child("config.toml")),
    );

    list.overflow_y_scroll()
}
