use gpui::{black, div, prelude::*, px, App, ClickEvent, FocusHandle, IntoElement, Window};

use crate::theme::Theme;

fn button(
    label: &'static str,
    theme: Theme,
    emphasize: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(label)
        .px(px(12.))
        .py(px(6.))
        .rounded(px(6.))
        .text_size(px(12.5))
        .cursor_pointer()
        .when(emphasize, |el| {
            el.bg(theme.accent)
                .text_color(theme.accent_ink)
                .font_weight(gpui::FontWeight::SEMIBOLD)
        })
        .when(!emphasize, |el| {
            el.text_color(theme.text_muted)
                .hover(|el| el.bg(theme.surface))
        })
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_click(event, window, cx);
        })
        .child(label)
}

/// A single-field text prompt (new/rename/duplicate request). The field's
/// value is driven externally — same `overlay_query` the command palette
/// and quick open already type into — so this component only renders it.
#[allow(clippy::too_many_arguments)]
pub fn render_prompt_modal(
    title: &str,
    placeholder: &str,
    value: &str,
    error: Option<&str>,
    confirm_label: &'static str,
    theme: Theme,
    focus_handle: &FocusHandle,
    on_confirm: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_dismiss: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id("prompt-modal-backdrop")
        .track_focus(focus_handle)
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .items_start()
        .pt(px(120.))
        .bg(black().opacity(0.55))
        .on_click(on_dismiss)
        .child(
            div()
                .id("prompt-modal-box")
                .w(px(420.))
                .bg(theme.surface_raised)
                .border_1()
                .border_color(theme.border)
                .rounded(px(10.))
                .shadow_lg()
                .p(px(16.))
                .on_click(|_event, _window, cx| cx.stop_propagation())
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(theme.text)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .mt(px(10.))
                        .px(px(10.))
                        .py(px(8.))
                        .bg(theme.surface)
                        .border_1()
                        .border_color(theme.border)
                        .rounded(px(6.))
                        .font_family("monospace")
                        .text_size(px(13.))
                        .child(if value.is_empty() {
                            div()
                                .text_color(theme.text_faint)
                                .child(placeholder.to_string())
                        } else {
                            div().text_color(theme.text).child(value.to_string())
                        }),
                )
                .when_some(error, |el, error| {
                    el.child(
                        div()
                            .mt(px(8.))
                            .text_size(px(11.5))
                            .text_color(theme.method_delete)
                            .child(error.to_string()),
                    )
                })
                .child(
                    div()
                        .mt(px(14.))
                        .flex()
                        .justify_end()
                        .items_center()
                        .gap(px(6.))
                        .child(button("Cancel", theme, false, on_cancel))
                        .child(button(confirm_label, theme, true, on_confirm)),
                ),
        )
}
