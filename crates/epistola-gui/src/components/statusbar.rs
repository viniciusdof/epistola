use gpui::{div, prelude::*, px, IntoElement, SharedString};

use crate::state::{ActiveFile, ActivityResult, AppState};
use crate::theme::Theme;

fn run_summary(activity: &ActivityResult) -> Option<String> {
    match activity {
        ActivityResult::Idle => None,
        ActivityResult::Running => Some("Sending…".to_string()),
        ActivityResult::RunSuccess {
            status,
            duration_ms,
            ..
        } => Some(format!("{status} · {duration_ms} ms")),
        ActivityResult::RunFailed(_) => Some("Run failed".to_string()),
        ActivityResult::Resolved(_) => Some("Resolved".to_string()),
        ActivityResult::ResolvedFailed(_) => Some("Resolve failed".to_string()),
        ActivityResult::Linted(_) => Some("Linted".to_string()),
        ActivityResult::LintFailed(_) => Some("Lint failed".to_string()),
    }
}

pub fn render_statusbar(state: &AppState, theme: Theme) -> impl IntoElement {
    let file_kind = match state.active_file {
        ActiveFile::None => "",
        ActiveFile::Config | ActiveFile::Request(_) => "TOML",
    };

    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(14.))
        .h(px(26.))
        .px(px(12.))
        .border_t_1()
        .border_color(theme.border)
        .text_size(px(11.))
        .text_color(theme.text_muted)
        .child(
            div()
                .flex()
                .items_center()
                .child(
                    div()
                        .flex_none()
                        .w(px(6.))
                        .h(px(6.))
                        .rounded(px(3.))
                        .bg(if state.environment.is_some() {
                            theme.success
                        } else {
                            theme.text_faint
                        })
                        .mr(px(5.)),
                )
                .child(SharedString::from(
                    state
                        .environment
                        .clone()
                        .unwrap_or_else(|| "no environment".to_string()),
                )),
        )
        .child(file_kind)
        .child("UTF-8")
        .child(div().flex_1())
        .when_some(run_summary(&state.activity), |el, summary| {
            el.child(summary)
        })
}
