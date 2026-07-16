use gpui::{div, prelude::*, px, IntoElement, Pixels};

use crate::actions::{ToggleDrawer, ToggleSidebar};
use crate::components::kit::{dispatch_on_click, IconName, TitlebarIconButton};
use crate::state::{ActivityResult, AppState};
use crate::theme::Theme;

pub const STATUSBAR_HEIGHT: Pixels = px(26.);

fn run_summary(activity: &ActivityResult) -> Option<String> {
    match activity {
        ActivityResult::Idle => None,
        ActivityResult::Running => Some("Sending…".to_string()),
        ActivityResult::RunSuccess(response) => Some(format!(
            "{} · {} ms",
            response.status,
            response.duration.as_millis()
        )),
        ActivityResult::RunFailed(_) => Some("Run failed".to_string()),
        ActivityResult::UnresolvedVariable { variable } => {
            Some(format!("Unresolved: {{{{{variable}}}}}"))
        }
        ActivityResult::Resolved(_) => Some("Resolved".to_string()),
        ActivityResult::ResolvedFailed(_) => Some("Resolve failed".to_string()),
        ActivityResult::Linted(_) => Some("Linted".to_string()),
        ActivityResult::LintFailed(_) => Some("Lint failed".to_string()),
    }
}

pub fn render_statusbar(state: &AppState, theme: Theme) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(STATUSBAR_HEIGHT)
        .px(px(10.))
        .border_t_1()
        .border_color(theme.border)
        .text_size(px(11.))
        .text_color(theme.text_muted)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(2.))
                .child(
                    TitlebarIconButton::new(IconName::PanelLeft, "Toggle Sidebar (⌘\\)")
                        .size(px(18.))
                        .active(!state.sidebar_collapsed)
                        .on_click(dispatch_on_click(ToggleSidebar)),
                )
                .child(
                    TitlebarIconButton::new(IconName::PanelBottom, "Toggle Response Panel (⌘J)")
                        .size(px(18.))
                        .active(!state.drawer_collapsed)
                        .on_click(dispatch_on_click(ToggleDrawer)),
                ),
        )
        .child(div().flex_1())
        .when_some(run_summary(state.active_activity()), |el, summary| {
            el.child(summary)
        })
}
