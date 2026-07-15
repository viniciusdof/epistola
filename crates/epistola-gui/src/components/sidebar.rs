use std::collections::HashSet;
use std::path::PathBuf;

use gpui::{div, prelude::*, px, uniform_list, Context, IntoElement, Pixels, SharedString};

use crate::actions::{
    OpenEnvironmentDoc, OpenFolderDoc, OpenRequestFile, OpenSettings, ToggleFolderCollapse,
};
use crate::collection::{CollectionTree, FolderEntry, RequestEntry};
use crate::components::kit::{dispatch_on_click, icon, IconName, MethodTag};
use crate::components::resize_handle::{resize_handle, ResizeAxis};
use crate::root::EpistolaGui;
use crate::state::{ActiveFile, AppState};
use crate::theme::Theme;

pub const SIDEBAR_WIDTH: Pixels = px(216.);

const ROW_HEIGHT: f32 = 24.;

#[derive(Clone)]
pub enum SidebarRow {
    Section(SharedString),
    Folder {
        name: SharedString,
        depth: usize,
        has_toml: bool,
        abs_path: PathBuf,
        collapsed: bool,
    },
    Request {
        entry: RequestEntry,
        depth: usize,
    },
    EmptyEnvironments,
    Environment(String),
    ConfigEntry,
    CollectionError(String),
}

fn flatten_folder(
    collection_root: &std::path::Path,
    folder: &FolderEntry,
    depth: usize,
    collapsed_folders: &HashSet<PathBuf>,
    out: &mut Vec<SidebarRow>,
) {
    let abs_path = collection_root.join(&folder.rel_path);
    let collapsed = collapsed_folders.contains(&abs_path);
    out.push(SidebarRow::Folder {
        name: folder.name.clone().into(),
        depth,
        has_toml: folder.has_folder_toml,
        abs_path: abs_path.clone(),
        collapsed,
    });
    if collapsed {
        return;
    }
    for request in &folder.requests {
        out.push(SidebarRow::Request {
            entry: request.clone(),
            depth: depth + 1,
        });
    }
    for child in &folder.folders {
        flatten_folder(collection_root, child, depth + 1, collapsed_folders, out);
    }
}

pub fn flatten(
    collection: &Result<CollectionTree, String>,
    collapsed_folders: &HashSet<PathBuf>,
) -> Vec<SidebarRow> {
    let mut rows = Vec::new();
    match collection {
        Ok(collection) => {
            rows.push(SidebarRow::Section(collection.name.clone().into()));
            for request in &collection.requests {
                rows.push(SidebarRow::Request {
                    entry: request.clone(),
                    depth: 1,
                });
            }
            for folder in &collection.folders {
                flatten_folder(&collection.root, folder, 0, collapsed_folders, &mut rows);
            }
            rows.push(SidebarRow::Section("environments".into()));
            if collection.environments.is_empty() {
                rows.push(SidebarRow::EmptyEnvironments);
            } else {
                for env in &collection.environments {
                    rows.push(SidebarRow::Environment(env.clone()));
                }
            }
        }
        Err(message) => {
            rows.push(SidebarRow::Section("collection".into()));
            rows.push(SidebarRow::CollectionError(message.clone()));
        }
    }
    rows.push(SidebarRow::Section("user".into()));
    rows.push(SidebarRow::ConfigEntry);
    rows
}

fn section_label(label: SharedString, theme: Theme) -> impl IntoElement {
    div()
        .h(px(ROW_HEIGHT))
        .flex()
        .items_end()
        .px(px(12.))
        .pb(px(4.))
        .text_size(px(10.5))
        .text_color(theme.text_faint)
        .child(label)
}

fn selectable_row(active: bool, indent: f32, theme: Theme) -> gpui::Div {
    div()
        .h(px(ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(6.))
        .pl(px(26. + indent))
        .pr(px(10.))
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

fn render_folder_row(
    name: SharedString,
    depth: usize,
    has_toml: bool,
    abs_path: &std::path::Path,
    collapsed: bool,
    active_folder: Option<&std::path::Path>,
    theme: Theme,
) -> impl IntoElement {
    let folder_active = has_toml && active_folder == Some(abs_path);
    let chevron_dir = abs_path.to_path_buf();
    let chevron = div()
        .id(SharedString::from(format!(
            "sidebar-folder-chevron-{}",
            abs_path.display()
        )))
        .cursor_pointer()
        .on_click(move |_event, window, cx| {
            cx.stop_propagation();
            window.dispatch_action(
                Box::new(ToggleFolderCollapse {
                    dir: chevron_dir.clone(),
                }),
                cx,
            );
        })
        .child(icon(
            if collapsed {
                IconName::ChevronRight
            } else {
                IconName::ChevronDown
            },
            px(10.),
            theme.text_faint,
        ));
    let mut row = div()
        .id(SharedString::from(format!(
            "sidebar-folder-{}",
            abs_path.display()
        )))
        .h(px(ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(6.))
        .pl(px(12. + depth as f32 * 14.))
        .pr(px(10.))
        .text_color(if folder_active {
            theme.accent
        } else {
            theme.text
        })
        .child(chevron)
        .child(div().child(name))
        .child(div().flex_1());
    if has_toml {
        let dir = abs_path.to_path_buf();
        row = row
            .cursor_pointer()
            .on_click(dispatch_on_click(OpenFolderDoc { dir }))
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
    row
}

struct ActiveMarkers {
    path: Option<PathBuf>,
    folder: Option<PathBuf>,
    environment: Option<String>,
    config: bool,
}

impl ActiveMarkers {
    fn from_state(state: &AppState) -> Self {
        Self {
            path: match &state.active_file {
                ActiveFile::Request(path) => Some(path.clone()),
                _ => None,
            },
            folder: match &state.active_file {
                ActiveFile::Folder(path) => Some(path.clone()),
                _ => None,
            },
            environment: match &state.active_file {
                ActiveFile::Environment(name) => Some(name.clone()),
                _ => None,
            },
            config: state.active_file == ActiveFile::Config,
        }
    }
}

fn render_row(row: &SidebarRow, active: &ActiveMarkers, theme: Theme) -> gpui::AnyElement {
    let active_path = active.path.as_deref();
    let active_folder = active.folder.as_deref();
    let active_environment = active.environment.as_deref();

    match row {
        SidebarRow::Section(label) => section_label(label.clone(), theme).into_any_element(),
        SidebarRow::Folder {
            name,
            depth,
            has_toml,
            abs_path,
            collapsed,
        } => render_folder_row(
            name.clone(),
            *depth,
            *has_toml,
            abs_path,
            *collapsed,
            active_folder,
            theme,
        )
        .into_any_element(),
        SidebarRow::Request { entry, depth } => {
            let active = active_path == Some(entry.abs_path.as_path());
            render_request_row(entry, active, *depth, theme).into_any_element()
        }
        SidebarRow::EmptyEnvironments => div()
            .h(px(ROW_HEIGHT))
            .flex()
            .items_center()
            .pl(px(26.))
            .pr(px(10.))
            .text_color(theme.text_faint)
            .child("none yet")
            .into_any_element(),
        SidebarRow::Environment(env) => {
            let active = active_environment == Some(env.as_str());
            let name = env.clone();
            selectable_row(active, 0., theme)
                .id(SharedString::from(format!("sidebar-env-{env}")))
                .on_click(dispatch_on_click(OpenEnvironmentDoc { name }))
                .child(env.clone())
                .into_any_element()
        }
        SidebarRow::ConfigEntry => selectable_row(active.config, 0., theme)
            .id("sidebar-config")
            .on_click(dispatch_on_click(OpenSettings))
            .child(div().flex_none().w(px(34.)).child(icon(
                IconName::Settings,
                px(12.),
                theme.text_muted,
            )))
            .child(div().child("config.toml"))
            .into_any_element(),
        SidebarRow::CollectionError(message) => div()
            .h(px(ROW_HEIGHT))
            .flex()
            .items_center()
            .px(px(12.))
            .text_size(px(11.5))
            .text_color(theme.text_faint)
            .child(format!("No collection here: {message}"))
            .into_any_element(),
    }
}

pub fn render_sidebar(state: &AppState, cx: &mut Context<EpistolaGui>) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
    let rows = state.sidebar_rows.clone();
    let count = rows.len();
    let active = ActiveMarkers::from_state(state);

    div()
        .id("sidebar")
        .flex()
        .flex_none()
        .w(state.sidebar_width)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .py(px(10.))
                .text_size(px(12.5))
                .child(
                    uniform_list("sidebar-rows", count, move |range, _window, _cx| {
                        range
                            .map(|i| render_row(&rows[i], &active, theme))
                            .collect()
                    })
                    .flex_1(),
                ),
        )
        .child(
            resize_handle("sidebar-resize-handle", ResizeAxis::Horizontal, theme).on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(EpistolaGui::start_sidebar_resize),
            ),
        )
}
