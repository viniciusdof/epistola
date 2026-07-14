use std::rc::Rc;

use gpui::{black, div, prelude::*, px, App, ClickEvent, IntoElement, Window};

use crate::theme::Theme;

pub type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

fn button(
    label: &'static str,
    theme: Theme,
    emphasize: bool,
    on_click: ClickHandler,
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

fn delete_button(label: &'static str, theme: Theme, on_click: ClickHandler) -> impl IntoElement {
    div()
        .id(label)
        .px(px(12.))
        .py(px(6.))
        .rounded(px(6.))
        .text_size(px(12.5))
        .cursor_pointer()
        .bg(theme.method_delete)
        .text_color(theme.ink)
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_click(event, window, cx);
        })
        .child(label)
}

/// Destructive confirmation (delete request): red title, single "Delete"
/// button instead of confirm_discard's neutral Save/Discard pair.
pub fn render_confirm_delete(
    title: &str,
    message: &str,
    error: Option<&str>,
    on_confirm: ClickHandler,
    on_cancel: ClickHandler,
    theme: Theme,
) -> impl IntoElement {
    let backdrop_cancel = on_cancel.clone();
    div()
        .id("confirm-delete-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .items_start()
        .pt(px(120.))
        .bg(black().opacity(0.55))
        .on_click(move |event, window, cx| backdrop_cancel(event, window, cx))
        .child(
            div()
                .id("confirm-delete-box")
                .w(px(360.))
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
                        .text_color(theme.method_delete)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .mt(px(6.))
                        .mb(px(14.))
                        .text_size(px(12.))
                        .text_color(theme.text_muted)
                        .child(message.to_string()),
                )
                .when_some(error, |el, error| {
                    el.child(
                        div()
                            .mb(px(10.))
                            .text_size(px(11.5))
                            .text_color(theme.method_delete)
                            .child(error.to_string()),
                    )
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .items_center()
                        .gap(px(6.))
                        .child(button("Cancel", theme, false, on_cancel))
                        .child(delete_button("Delete", theme, on_confirm)),
                ),
        )
}

pub fn render_confirm_discard(
    message: &str,
    on_save: Option<ClickHandler>,
    on_discard: ClickHandler,
    on_cancel: ClickHandler,
    theme: Theme,
) -> impl IntoElement {
    let backdrop_cancel = on_cancel.clone();
    div()
        .id("confirm-discard-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .items_start()
        .pt(px(120.))
        .bg(black().opacity(0.55))
        .on_click(move |event, window, cx| backdrop_cancel(event, window, cx))
        .child(
            div()
                .id("confirm-discard-box")
                .w(px(360.))
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
                        .child("Unsaved changes"),
                )
                .child(
                    div()
                        .mt(px(6.))
                        .mb(px(14.))
                        .text_size(px(12.))
                        .text_color(theme.text_muted)
                        .child(message.to_string()),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .items_center()
                        .gap(px(6.))
                        .child(button("Cancel", theme, false, on_cancel))
                        .child(button("Discard", theme, false, on_discard))
                        .children(on_save.map(|on_save| button("Save", theme, true, on_save))),
                ),
        )
}
