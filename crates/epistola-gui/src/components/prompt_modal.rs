use gpui::{black, div, prelude::*, px, App, ClickEvent, Entity, IntoElement, Window};

use crate::components::kit::button;
use crate::text_field::TextField;
use crate::theme::Theme;

/// A single-field text prompt (new/rename/duplicate request).
#[allow(clippy::too_many_arguments)]
pub fn render_prompt_modal(
    title: &str,
    input: &Entity<TextField>,
    error: Option<&str>,
    confirm_label: &'static str,
    theme: Theme,
    on_confirm: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_dismiss: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id("prompt-modal-backdrop")
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
                        .child(input.clone()),
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
