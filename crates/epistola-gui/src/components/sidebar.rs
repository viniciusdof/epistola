use std::path::Path;

use gpui::{div, prelude::*, px, Context, IntoElement, SharedString};

use crate::actions::{OpenEnvironmentDoc, OpenFolderDoc, OpenRequestFile, OpenSettings};
use crate::collection::{FolderEntry, RequestEntry};
use crate::components::kit::{dispatch_on_click, icon, IconName, MethodTag};
use crate::root::EpistolaGui;
use crate::state::{ActiveFile, AppState};
use crate::theme::Theme;

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

fn render_request_row(
    request: &RequestEntry,
    active: bool,
    depth: usize,
    theme: Theme,
) -> impl IntoElement {
    let path = request.abs_path.clone();
    selectable_row(active, depth as f32 * 14., theme)
        .id(SharedString::from(format!(
            "sidebar-request-{}",
            request.rel_path.display()
        )))
        .on_click(dispatch_on_click(OpenRequestFile { path }))
        .aria_label(SharedString::from(request.display_name.clone()))
        .child(MethodTag::new(request.method.clone()))
        .child(
            div()
                .overflow_hidden()
                .text_ellipsis()
                .child(request.file_name.clone()),
        )
}

fn render_folder_rows(
    collection_root: &Path,
    folder: &FolderEntry,
    depth: usize,
    active_path: Option<&Path>,
    active_folder: Option<&Path>,
    theme: Theme,
    out: &mut Vec<gpui::AnyElement>,
) {
    let folder_abs_path = collection_root.join(&folder.rel_path);
    let folder_active = folder.has_folder_toml && active_folder == Some(folder_abs_path.as_path());
    let mut row = div()
        .id(SharedString::from(format!(
            "sidebar-folder-{}",
            folder.rel_path.display()
        )))
        .flex()
        .items_center()
        .gap(px(6.))
        .pl(px(12. + depth as f32 * 14.))
        .pr(px(10.))
        .py(px(4.))
        .text_color(if folder_active {
            theme.accent
        } else {
            theme.text
        })
        .child(icon(IconName::ChevronDown, px(10.), theme.text_faint))
        .child(div().child(folder.name.clone()))
        .child(div().flex_1());
    if folder.has_folder_toml {
        row = row
            .cursor_pointer()
            .on_click(dispatch_on_click(OpenFolderDoc {
                dir: folder_abs_path,
            }))
            .child(
                div()
                    .text_size(px(9.5))
                    .text_color(theme.text_faint)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(3.))
                    .px(px(4.))
                    .child("ƒ"),
            );
    }
    out.push(row.into_any_element());

    for request in &folder.requests {
        let active = active_path == Some(request.abs_path.as_path());
        out.push(render_request_row(request, active, depth + 1, theme).into_any_element());
    }
    for child in &folder.folders {
        render_folder_rows(
            collection_root,
            child,
            depth + 1,
            active_path,
            active_folder,
            theme,
            out,
        );
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

pub fn render_sidebar(state: &AppState, cx: &mut Context<EpistolaGui>) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
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
    let active_folder = match &state.active_file {
        ActiveFile::Folder(path) => Some(path.as_path()),
        _ => None,
    };
    let active_environment = match &state.active_file {
        ActiveFile::Environment(name) => Some(name.as_str()),
        _ => None,
    };

    match &state.collection {
        Ok(collection) => {
            list = list.child(section_label(collection.name.clone(), theme));

            let mut rows: Vec<gpui::AnyElement> = Vec::new();
            for request in &collection.requests {
                let active = active_path == Some(request.abs_path.as_path());
                rows.push(render_request_row(request, active, 1, theme).into_any_element());
            }
            for folder in &collection.folders {
                render_folder_rows(
                    &collection.root,
                    folder,
                    0,
                    active_path,
                    active_folder,
                    theme,
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
                    let active = active_environment == Some(env.as_str());
                    let name = env.clone();
                    list = list.child(
                        selectable_row(active, 0., theme)
                            .id(SharedString::from(format!("sidebar-env-{env}")))
                            .on_click(dispatch_on_click(OpenEnvironmentDoc { name }))
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
    list = list.child(
        selectable_row(config_active, 0., theme)
            .id("sidebar-config")
            .on_click(dispatch_on_click(OpenSettings))
            .child(div().flex_none().w(px(34.)).child(icon(
                IconName::Settings,
                px(12.),
                theme.text_muted,
            )))
            .child(div().child("config.toml")),
    );

    list.overflow_y_scroll()
}
