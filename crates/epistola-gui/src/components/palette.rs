use std::rc::Rc;

use gpui::{div, prelude::*, px, App, ClickEvent, IntoElement, SharedString, Window};

use crate::theme::Theme;

pub type SelectHandler = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct PaletteItem {
    pub leading: Option<gpui::AnyElement>,
    pub label: SharedString,
    pub shortcut: Option<SharedString>,
    pub on_select: SelectHandler,
}

impl PaletteItem {
    pub fn new(label: impl Into<SharedString>, on_select: SelectHandler) -> Self {
        Self {
            leading: None,
            label: label.into(),
            shortcut: None,
            on_select,
        }
    }

    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading = Some(element.into_any_element());
        self
    }
}

fn render_item(item: PaletteItem, theme: Theme, index: usize) -> impl IntoElement {
    let on_select = item.on_select.clone();
    div()
        .id(SharedString::from(format!("palette-item-{index}")))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .px(px(10.))
        .py(px(8.))
        .rounded(px(6.))
        .text_size(px(13.))
        .text_color(theme.text)
        .cursor_pointer()
        .hover(|el| el.bg(theme.accent).text_color(theme.accent_ink))
        .on_click(move |_event, window, cx| {
            cx.stop_propagation();
            on_select(window, cx);
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .children(item.leading)
                .child(item.label),
        )
        .when_some(item.shortcut, |el, shortcut| {
            el.child(
                div()
                    .font_family("monospace")
                    .text_size(px(11.))
                    .text_color(theme.text_faint)
                    .child(shortcut),
            )
        })
}

pub fn render_palette_overlay(
    placeholder: &'static str,
    items: Vec<PaletteItem>,
    theme: Theme,
    on_dismiss: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let empty = items.is_empty();
    div()
        .id("palette-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .items_start()
        .pt(px(96.))
        .bg(gpui::black().opacity(0.55))
        .on_click(on_dismiss)
        .child(
            div()
                .id("palette-box")
                .w(px(560.))
                .max_w(px(560.))
                .bg(theme.surface_raised)
                .border_1()
                .border_color(theme.border)
                .rounded(px(10.))
                .shadow_lg()
                .on_click(|_event, _window, cx| cx.stop_propagation())
                .child(
                    div()
                        .px(px(16.))
                        .py(px(14.))
                        .border_b_1()
                        .border_color(theme.border)
                        .font_family("monospace")
                        .text_size(px(14.5))
                        .text_color(theme.text_faint)
                        .child(placeholder),
                )
                .child(
                    div()
                        .id("palette-list")
                        .p(px(6.))
                        .max_h(px(320.))
                        .overflow_y_scroll()
                        .children(
                            items
                                .into_iter()
                                .enumerate()
                                .map(|(i, item)| render_item(item, theme, i)),
                        ),
                )
                .when(empty, |el| {
                    el.child(
                        div()
                            .py(px(18.))
                            .text_align(gpui::TextAlign::Center)
                            .text_color(theme.text_faint)
                            .text_size(px(12.5))
                            .child("No matches"),
                    )
                }),
        )
}
