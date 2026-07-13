use gpui::{div, prelude::*, px, IntoElement};

use crate::components::kit::{ClickHandler, IconName, RailButton};
use crate::state::{ActiveFile, AppState, View};
use crate::theme::Theme;

pub struct RailCallbacks {
    pub on_home: ClickHandler,
    pub on_workspace: ClickHandler,
    pub on_environments: ClickHandler,
    pub on_history: ClickHandler,
    pub on_settings: ClickHandler,
}

pub fn render_activity_rail(
    state: &AppState,
    theme: Theme,
    callbacks: RailCallbacks,
) -> impl IntoElement {
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
        .child(
            RailButton::new(IconName::Layers, "Environments", theme)
                .on_click(callbacks.on_environments),
        )
        .child(RailButton::new(IconName::History, "History", theme).on_click(callbacks.on_history))
        .child(div().flex_1())
        .child(
            RailButton::new(IconName::Settings, "Settings (⌘,)", theme)
                .active(settings_active)
                .on_click(callbacks.on_settings),
        )
}
