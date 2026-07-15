use epistola_core::Method;
use gpui::{
    div, prelude::*, px, uniform_list, Action, Context, Entity, IntoElement, SharedString,
    UniformListScrollHandle,
};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};

use crate::actions::Dismiss;
use crate::components::kit::{dispatch_on_click, MethodTag};
use crate::root::EpistolaGui;
use crate::text_field::TextField;
use crate::theme::Theme;

const ROW_HEIGHT: f32 = 34.;
const MAX_VISIBLE_ROWS: usize = 8;

#[derive(Clone)]
pub enum PickerLeading {
    Method(Method),
    Dot { active: bool },
}

pub struct PickerItem {
    pub leading: Option<PickerLeading>,
    pub label: SharedString,
    pub detail: Option<SharedString>,
    pub action: Box<dyn Action>,
    pub closes_overlay: bool,
}

impl Clone for PickerItem {
    fn clone(&self) -> Self {
        Self {
            leading: self.leading.clone(),
            label: self.label.clone(),
            detail: self.detail.clone(),
            action: self.action.boxed_clone(),
            closes_overlay: self.closes_overlay,
        }
    }
}

impl PickerItem {
    pub fn new(label: impl Into<SharedString>, action: impl Action) -> Self {
        Self {
            leading: None,
            label: label.into(),
            detail: None,
            action: Box::new(action),
            closes_overlay: true,
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn leading_method(mut self, method: Method) -> Self {
        self.leading = Some(PickerLeading::Method(method));
        self
    }

    pub fn leading_dot(mut self, active: bool) -> Self {
        self.leading = Some(PickerLeading::Dot { active });
        self
    }

    pub fn keep_overlay_open(mut self) -> Self {
        self.closes_overlay = false;
        self
    }
}

pub fn filter_items(matcher: &mut Matcher, items: Vec<PickerItem>, query: &str) -> Vec<PickerItem> {
    if query.trim().is_empty() {
        return items;
    }
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, PickerItem)> = items
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

fn dot(active: bool, theme: Theme) -> impl IntoElement {
    div()
        .flex_none()
        .w(px(6.))
        .h(px(6.))
        .rounded(px(3.))
        .bg(if active {
            theme.success
        } else {
            theme.text_faint
        })
}

fn render_row(
    item: &PickerItem,
    index: usize,
    is_selected: bool,
    theme: Theme,
) -> impl IntoElement {
    let action = item.action.boxed_clone();
    let closes_overlay = item.closes_overlay;
    div()
        .id(SharedString::from(format!("picker-item-{index}")))
        .w_full()
        .h(px(ROW_HEIGHT))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .px(px(10.))
        .mx(px(6.))
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
            window.dispatch_action(action.boxed_clone(), cx);
            if closes_overlay {
                window.dispatch_action(Box::new(Dismiss), cx);
            }
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .when_some(item.leading.clone(), |el, leading| {
                    el.child(match leading {
                        PickerLeading::Method(method) => MethodTag::new(method).into_any_element(),
                        PickerLeading::Dot { active } => dot(active, theme).into_any_element(),
                    })
                })
                .child(item.label.clone()),
        )
        .when_some(item.detail.clone(), |el, detail| {
            el.child(
                div()
                    .font_family("monospace")
                    .text_size(px(11.))
                    .text_color(theme.text_faint)
                    .child(detail),
            )
        })
}

#[allow(clippy::too_many_arguments)]
pub fn render_picker(
    input: &Entity<TextField>,
    items: &[PickerItem],
    selected: usize,
    scroll: &UniformListScrollHandle,
    theme: Theme,
    cx: &mut Context<EpistolaGui>,
) -> impl IntoElement {
    let empty = items.is_empty();
    let count = items.len();
    let rows: Vec<PickerItem> = items.to_vec();
    let scroll = scroll.clone();
    let list_height = px(ROW_HEIGHT * count.clamp(1, MAX_VISIBLE_ROWS) as f32 + 12.);

    div()
        .id("picker-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .items_start()
        .pt(px(96.))
        .bg(gpui::black().opacity(0.55))
        .on_click(dispatch_on_click(Dismiss))
        .child(
            div()
                .id("picker-box")
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
                        .py(px(10.))
                        .border_b_1()
                        .border_color(theme.border)
                        .font_family("monospace")
                        .text_size(px(14.5))
                        .child(input.clone()),
                )
                .when(empty, |el| {
                    let query = input.read(cx).text().to_string();
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
                })
                .when(!empty, |el| {
                    el.child(
                        div().p(px(6.)).child(
                            uniform_list("picker-rows", count, move |range, _window, _cx| {
                                range
                                    .map(|i| {
                                        render_row(&rows[i], i, i == selected, theme)
                                            .into_any_element()
                                    })
                                    .collect()
                            })
                            .h(list_height)
                            .w_full()
                            .track_scroll(&scroll),
                        ),
                    )
                }),
        )
}
