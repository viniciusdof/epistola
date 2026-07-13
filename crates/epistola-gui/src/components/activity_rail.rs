use gpui::{div, prelude::*, px, App, ClickEvent, IntoElement, Window};

use crate::components::kit::{IconName, RailButton};
use crate::state::{ActiveFile, AppState, View};
use crate::theme::Theme;

pub struct RailCallbacks<H, W, S>
where
    H: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    W: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    S: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    pub on_home: H,
    pub on_workspace: W,
    pub on_settings: S,
}

pub fn render_activity_rail<H, W, S>(
    state: &AppState,
    theme: Theme,
    callbacks: RailCallbacks<H, W, S>,
) -> impl IntoElement
where
    H: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    W: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    S: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let settings_active = state.view == View::Workspace && state.active_file == ActiveFile::Config;
    let workspace_active = state.view == View::Workspace && !settings_active;

    div()
        .flex()
        .flex_none()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .w(px(44.))
        .py(px(10.))
        .border_r_1()
        .border_color(theme.border)
        .child(
            RailButton::new(IconName::Home, "Home (⌘0)", theme)
                .active(state.view == View::Home)
                .on_click(callbacks.on_home),
        )
        .child(
            RailButton::new(IconName::FolderTree, "Collection", theme)
                .active(workspace_active)
                .on_click(callbacks.on_workspace),
        )
        .child(RailButton::new(IconName::Layers, "Environments", theme))
        .child(RailButton::new(IconName::History, "History", theme))
        .child(div().flex_1())
        .child(
            RailButton::new(IconName::Settings, "Settings (⌘,)", theme)
                .active(settings_active)
                .on_click(callbacks.on_settings),
        )
}
