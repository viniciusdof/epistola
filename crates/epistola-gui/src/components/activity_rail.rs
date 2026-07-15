use gpui::{div, prelude::*, px, Context, IntoElement, Pixels};

use crate::actions::{
    GoHome, GoWorkspace, OpenEnvironmentPicker, OpenHistory, OpenSettings, ToggleSidebar,
};
use crate::components::kit::{dispatch_on_click, IconName, RailButton};
use crate::root::EpistolaGui;
use crate::state::{ActiveFile, AppState, View};
use crate::theme::Theme;

pub const RAIL_WIDTH: Pixels = px(44.);

pub fn render_activity_rail(state: &AppState, cx: &mut Context<EpistolaGui>) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
    let settings_active = state.view == View::Workspace && state.active_file == ActiveFile::Config;
    let workspace_active = state.view == View::Workspace && !settings_active;

    div()
        .flex()
        .flex_none()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .w(RAIL_WIDTH)
        .py(px(10.))
        .border_r_1()
        .border_color(theme.border)
        .child(
            RailButton::new(IconName::Home, "Home (⌘0)")
                .active(state.view == View::Home)
                .on_click(dispatch_on_click(GoHome)),
        )
        .child(
            RailButton::new(IconName::FolderTree, "Collection")
                .active(workspace_active)
                .on_click(move |_event, window, cx| {
                    if workspace_active {
                        window.dispatch_action(Box::new(ToggleSidebar), cx);
                    } else {
                        window.dispatch_action(Box::new(GoWorkspace), cx);
                    }
                }),
        )
        .child(
            RailButton::new(IconName::Layers, "Environments")
                .on_click(dispatch_on_click(OpenEnvironmentPicker)),
        )
        .child(
            RailButton::new(IconName::History, "History").on_click(dispatch_on_click(OpenHistory)),
        )
        .child(div().flex_1())
        .child(
            RailButton::new(IconName::Settings, "Settings (⌘,)")
                .active(settings_active)
                .on_click(dispatch_on_click(OpenSettings)),
        )
}
