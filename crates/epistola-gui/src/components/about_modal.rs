use gpui::{div, prelude::*, px, App, ClickEvent, ClipboardItem, IntoElement, Window};

use crate::build_info::{BuildInfo, BUILD_INFO};
use crate::theme::Theme;

pub fn render_about_modal(
    theme: Theme,
    on_dismiss: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let headline_text = headline(&BUILD_INFO);
    let clipboard_text = copy_text(&BUILD_INFO);

    div()
        .id("about-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .items_center()
        .bg(gpui::black().opacity(0.55))
        .on_click(on_dismiss)
        .child(
            div()
                .id("about-box")
                .w(px(360.))
                .bg(theme.surface_raised)
                .border_1()
                .border_color(theme.border)
                .rounded(px(10.))
                .shadow_lg()
                .p(px(20.))
                .on_click(|_event, _window, cx| cx.stop_propagation())
                .child(
                    div()
                        .font_family("monospace")
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_size(px(20.))
                        .text_color(theme.accent)
                        .child("ϵpistola"),
                )
                .child(
                    div()
                        .pt(px(4.))
                        .text_size(px(13.))
                        .text_color(theme.text)
                        .child(headline_text),
                )
                .child(about_row(theme, "Commit", BUILD_INFO.git_sha.to_string()))
                .child(about_row(theme, "Target", BUILD_INFO.target.to_string()))
                .child(
                    div().pt(px(14.)).flex().justify_end().child(
                        div()
                            .id("about-copy")
                            .px(px(10.))
                            .py(px(5.))
                            .rounded(px(6.))
                            .bg(theme.surface)
                            .cursor_pointer()
                            .text_size(px(12.))
                            .text_color(theme.text_muted)
                            .on_click(
                                move |_event: &ClickEvent, _window: &mut Window, cx: &mut App| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        clipboard_text.clone(),
                                    ));
                                },
                            )
                            .child("Copy"),
                    ),
                ),
        )
}

fn about_row(theme: Theme, label: &'static str, value: String) -> impl IntoElement {
    div()
        .pt(px(6.))
        .flex()
        .justify_between()
        .text_size(px(11.5))
        .child(div().text_color(theme.text_faint).child(label))
        .child(
            div()
                .font_family("monospace")
                .text_color(theme.text_muted)
                .child(value),
        )
}

fn headline(info: &BuildInfo) -> String {
    let dirty = if info.is_dirty() { " (dirty)" } else { "" };
    if info.is_nightly() {
        format!("nightly · {}{dirty}", info.git_date)
    } else {
        format!("v{}{dirty}", info.version)
    }
}

fn copy_text(info: &BuildInfo) -> String {
    format!(
        "epistola {} ({}, {}) [{}]",
        info.version, info.channel, info.git_sha, info.target
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nightly_headline_omits_semver() {
        let info = BuildInfo {
            version: "0.1.0",
            channel: "nightly",
            git_sha: "abc1234",
            git_date: "2026-07-27",
            dirty: "clean",
            target: "t",
        };
        assert_eq!(headline(&info), "nightly · 2026-07-27");
    }

    #[test]
    fn dirty_dev_headline_shows_semver_and_dirty_marker() {
        let info = BuildInfo {
            version: "0.1.0",
            channel: "dev",
            git_sha: "abc1234",
            git_date: "2026-07-27",
            dirty: "dirty",
            target: "t",
        };
        assert_eq!(headline(&info), "v0.1.0 (dirty)");
    }
}
