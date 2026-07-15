use std::rc::Rc;

use epistola_core::Method;
use gpui::{div, prelude::*, px, App, ClickEvent, FocusHandle, IntoElement, SharedString, Window};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};

use crate::components::kit::MethodTag;
use crate::theme::Theme;

pub type SelectHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone)]
pub struct PaletteItem {
    pub leading_method: Option<Method>,
    pub label: SharedString,
    pub shortcut: Option<SharedString>,
    pub on_select: SelectHandler,
}

impl PaletteItem {
    pub fn new(label: impl Into<SharedString>, on_select: SelectHandler) -> Self {
        Self {
            leading_method: None,
            label: label.into(),
            shortcut: None,
            on_select,
        }
    }

    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn leading_method(mut self, method: Method) -> Self {
        self.leading_method = Some(method);
        self
    }
}

pub fn filter_items(
    matcher: &mut Matcher,
    items: Vec<PaletteItem>,
    query: &str,
) -> Vec<PaletteItem> {
    if query.trim().is_empty() {
        return items;
    }
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, PaletteItem)> = items
        .into_iter()
        .filter_map(|item| {
            let haystack = Utf32Str::new(&item.label, &mut buf);
            let score = pattern.score(haystack, matcher)?;
            Some((score, item))
        })
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, item)| item).collect()
}

fn render_item(
    item: &PaletteItem,
    theme: Theme,
    index: usize,
    is_selected: bool,
) -> impl IntoElement {
    let on_select = item.on_select.clone();
    let leading = item
        .leading_method
        .clone()
        .map(|method| MethodTag::new(method, theme));
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
        .when(is_selected, |el| {
            el.bg(theme.accent).text_color(theme.accent_ink)
        })
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
                .children(leading)
                .child(item.label.clone()),
        )
        .when_some(item.shortcut.clone(), |el, shortcut| {
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
    query: &str,
    items: &[PaletteItem],
    selected: usize,
    theme: Theme,
    focus_handle: &FocusHandle,
    on_dismiss: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let empty = items.is_empty();
    div()
        .id("palette-backdrop")
        .track_focus(focus_handle)
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
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .px(px(16.))
                        .py(px(14.))
                        .border_b_1()
                        .border_color(theme.border)
                        .font_family("monospace")
                        .text_size(px(14.5))
                        .child(if query.is_empty() {
                            div().text_color(theme.text_faint).child(placeholder)
                        } else {
                            div().text_color(theme.text).child(query.to_string())
                        })
                        .when(!query.is_empty(), |el| {
                            el.child(div().w(px(2.)).h(px(17.)).bg(theme.accent))
                        }),
                )
                .child(
                    div()
                        .id("palette-list")
                        .p(px(6.))
                        .max_h(px(320.))
                        .overflow_y_scroll()
                        .children(
                            items
                                .iter()
                                .enumerate()
                                .map(|(i, item)| render_item(item, theme, i, i == selected)),
                        ),
                )
                .when(empty, |el| {
                    let message = if query.is_empty() {
                        "No matches".to_string()
                    } else {
                        format!("No matches for '{query}'")
                    };
                    el.child(
                        div()
                            .py(px(18.))
                            .text_align(gpui::TextAlign::Center)
                            .text_color(theme.text_faint)
                            .text_size(px(12.5))
                            .child(message),
                    )
                }),
        )
}
