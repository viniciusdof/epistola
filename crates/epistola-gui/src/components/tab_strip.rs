use gpui::{div, prelude::*, px, Context, IntoElement, SharedString};

use crate::actions::{CloseTab, SwitchTab};
use crate::components::kit::{dispatch_on_click, icon, IconName, MethodTag};
use crate::root::EpistolaGui;
use crate::state::{ActiveFile, AppState};
use crate::theme::Theme;

fn tab_glyph(state: &AppState, theme: Theme, file: &ActiveFile) -> gpui::AnyElement {
    match file {
        ActiveFile::None => div().into_any_element(),
        ActiveFile::Config => {
            icon(IconName::Settings, px(12.), theme.text_muted).into_any_element()
        }
        ActiveFile::Folder(_) => div()
            .flex_none()
            .text_size(px(10.))
            .text_color(theme.text_faint)
            .child("ƒ")
            .into_any_element(),
        ActiveFile::Environment(_) => div()
            .flex_none()
            .w(px(6.))
            .h(px(6.))
            .rounded(px(3.))
            .bg(theme.success)
            .into_any_element(),
        ActiveFile::Request(path) => match state
            .collection
            .as_ref()
            .ok()
            .and_then(|c| c.find_request(path))
        {
            Some(request) => MethodTag::new(request.method.clone()).into_any_element(),
            None => div().into_any_element(),
        },
    }
}

fn tab_text(state: &AppState, file: &ActiveFile) -> SharedString {
    match file {
        ActiveFile::None => "".into(),
        ActiveFile::Config => "config.toml".into(),
        ActiveFile::Folder(dir) => match dir.file_name() {
            Some(name) => format!("{}/folder.toml", name.to_string_lossy()).into(),
            None => "folder.toml".into(),
        },
        ActiveFile::Environment(name) => format!("{name}.toml").into(),
        ActiveFile::Request(path) => match state
            .collection
            .as_ref()
            .ok()
            .and_then(|c| c.find_request(path))
        {
            Some(request) => request.file_name.clone().into(),
            None => path.display().to_string().into(),
        },
    }
}

pub fn render_tab_strip(state: &AppState, cx: &mut Context<EpistolaGui>) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
    div()
        .flex_none()
        .w_full()
        .overflow_x_hidden()
        .border_b_1()
        .border_color(theme.border)
        .child(render_tabs(state, theme))
}

fn render_tabs(state: &AppState, theme: Theme) -> impl IntoElement {
    div()
        .id("tab-strip")
        .flex()
        .flex_none()
        .min_w(px(0.))
        .overflow_x_scroll()
        .children(state.open_tabs.iter().map(|file| {
            let active = file == &state.active_file;
            let select_file = file.clone();
            let close_file = file.clone();
            div()
                .id(SharedString::from(format!("tab-{file:?}")))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(7.))
                .px(px(10.))
                .py(px(8.))
                .border_r_1()
                .border_color(theme.border)
                .bg(if active { theme.ink } else { theme.surface })
                .text_size(px(12.5))
                .text_color(if active { theme.text } else { theme.text_muted })
                .cursor_pointer()
                .on_click(dispatch_on_click(SwitchTab { file: select_file }))
                .child(tab_glyph(state, theme, file))
                .child(
                    div()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(tab_text(state, file)),
                )
                .when(state.is_dirty(file), |el| {
                    el.child(
                        div()
                            .flex_none()
                            .w(px(6.))
                            .h(px(6.))
                            .rounded(px(3.))
                            .bg(theme.accent),
                    )
                })
                .child(
                    div()
                        .id(SharedString::from(format!("tab-close-{file:?}")))
                        .flex()
                        .flex_none()
                        .w(px(14.))
                        .h(px(14.))
                        .items_center()
                        .justify_center()
                        .rounded(px(3.))
                        .cursor_pointer()
                        .hover(|el| el.bg(theme.border))
                        .on_click(move |_event, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(
                                Box::new(CloseTab {
                                    file: close_file.clone(),
                                }),
                                cx,
                            );
                        })
                        .child(icon(IconName::Close, px(9.), theme.text_faint)),
                )
        }))
}
