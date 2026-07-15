use epistola_core::Method;
use gpui::{
    div, prelude::*, px, svg, AnyView, App, ClickEvent, Context, Hsla, IntoElement, Pixels, Render,
    RenderOnce, SharedString, Svg, Window,
};

use crate::theme::Theme;

pub type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub fn dispatch_on_click<A: gpui::Action + Clone>(
    action: A,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_event, window, cx| window.dispatch_action(Box::new(action.clone()), cx)
}

/// A confirm/cancel-style text button.
pub fn button(
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

struct TooltipLabel {
    text: SharedString,
}

impl Render for TooltipLabel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        div()
            .px(px(8.))
            .py(px(5.))
            .bg(theme.surface_raised)
            .border_1()
            .border_color(theme.border)
            .rounded(px(5.))
            .text_size(px(11.5))
            .text_color(theme.text)
            .child(self.text.clone())
    }
}

/// A plain-text tooltip builder for `div().tooltip(...)`.
pub fn tooltip_text(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_window, cx| cx.new(|_| TooltipLabel { text: text.clone() }).into()
}

/// Lucide icons vendored under `assets/icons/`, served through
/// `crate::assets::Assets` (an `AssetSource` impl registered on the `App`).
#[derive(Clone, Copy)]
pub enum IconName {
    Home,
    FolderTree,
    Layers,
    History,
    Settings,
    ChevronDown,
    ChevronRight,
    Info,
    Close,
    Loading,
}

impl IconName {
    fn path(self) -> &'static str {
        match self {
            IconName::Home => "icons/house.svg",
            IconName::FolderTree => "icons/folder-tree.svg",
            IconName::Layers => "icons/layers.svg",
            IconName::History => "icons/history.svg",
            IconName::Settings => "icons/settings.svg",
            IconName::ChevronDown => "icons/chevron-down.svg",
            IconName::ChevronRight => "icons/chevron-right.svg",
            IconName::Info => "icons/info.svg",
            IconName::Close => "icons/x.svg",
            IconName::Loading => "icons/loader-circle.svg",
        }
    }
}

pub fn icon(name: IconName, size: Pixels, color: Hsla) -> Svg {
    svg()
        .path(name.path())
        .size(size)
        .text_color(color)
        .flex_none()
}

#[derive(IntoElement)]
pub struct MethodTag {
    method: Method,
}

impl MethodTag {
    pub fn new(method: Method) -> Self {
        Self { method }
    }
}

impl RenderOnce for MethodTag {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        div()
            .flex_none()
            .w(px(34.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_size(px(9.5))
            .text_color(theme.method_color(&self.method))
            .child(self.method.as_str().to_string())
    }
}

#[derive(IntoElement)]
pub struct KbdChip {
    label: SharedString,
}

impl KbdChip {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl RenderOnce for KbdChip {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        div()
            .font_family("monospace")
            .text_size(px(10.5))
            .text_color(theme.text_muted)
            .bg(theme.surface_raised)
            .border_1()
            .border_color(theme.border)
            .rounded(px(3.))
            .px(px(4.))
            .child(self.label)
    }
}

#[derive(IntoElement)]
pub struct TitlebarButton {
    label: SharedString,
    shortcut: SharedString,
    on_click: Option<ClickHandler>,
}

impl TitlebarButton {
    pub fn new(label: impl Into<SharedString>, shortcut: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            shortcut: shortcut.into(),
            on_click: None,
        }
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for TitlebarButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        div()
            .id(SharedString::from(format!("titlebar-btn-{}", self.label)))
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(9.))
            .py(px(4.))
            .rounded(px(5.))
            .border_1()
            .border_color(theme.border)
            .text_size(px(12.))
            .text_color(theme.text)
            .cursor_pointer()
            .hover(|el| el.border_color(theme.accent).text_color(theme.accent))
            .when_some(self.on_click, |el, handler| el.on_click(handler))
            .child(self.label)
            .child(KbdChip::new(self.shortcut).render(window, cx))
    }
}

/// One icon button in the left activity rail. Buttons without an `on_click`
/// render as inert (used for not-yet-implemented entries).
#[derive(IntoElement)]
pub struct RailButton {
    icon_name: IconName,
    tooltip: SharedString,
    active: bool,
    on_click: Option<ClickHandler>,
}

impl RailButton {
    pub fn new(icon_name: IconName, tooltip: impl Into<SharedString>) -> Self {
        Self {
            icon_name,
            tooltip: tooltip.into(),
            active: false,
            on_click: None,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for RailButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        let clickable = self.on_click.is_some();
        div()
            .id(SharedString::from(format!("rail-{}", self.tooltip)))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(px(30.))
            .h(px(30.))
            .rounded(px(6.))
            .when(self.active, |el| el.bg(theme.surface_raised))
            .when(clickable, |el| {
                el.cursor_pointer().hover(|el| el.bg(theme.surface_raised))
            })
            .when_some(self.on_click, |el, handler| el.on_click(handler))
            .aria_label(self.tooltip.clone())
            .tooltip(tooltip_text(self.tooltip))
            .child(icon(
                self.icon_name,
                px(16.),
                if self.active {
                    theme.accent
                } else {
                    theme.text_muted
                },
            ))
    }
}

pub fn dot_pill(label: impl Into<SharedString>, dot_color: Hsla, theme: Theme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(5.))
        .px(px(8.))
        .py(px(2.))
        .rounded(px(4.))
        .bg(theme.surface_raised)
        .border_1()
        .border_color(theme.border)
        .text_size(px(11.5))
        .text_color(theme.text_muted)
        .child(
            div()
                .flex_none()
                .w(px(6.))
                .h(px(6.))
                .rounded(px(3.))
                .bg(dot_color),
        )
        .child(label.into())
}
