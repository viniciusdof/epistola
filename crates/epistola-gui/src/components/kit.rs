use std::path::PathBuf;
use std::rc::Rc;

use epistola_core::Method;
use gpui::{
    div, prelude::*, px, svg, App, ClickEvent, Hsla, IntoElement, Pixels, RenderOnce, SharedString,
    Svg, Window,
};

use crate::theme::Theme;

pub type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub type PathClickHandler = Rc<dyn Fn(PathBuf, &mut Window, &mut App)>;

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
    Info,
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
            IconName::Info => "icons/info.svg",
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
    theme: Theme,
}

impl MethodTag {
    pub fn new(method: Method, theme: Theme) -> Self {
        Self { method, theme }
    }
}

impl RenderOnce for MethodTag {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex_none()
            .w(px(34.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_size(px(9.5))
            .text_color(self.theme.method_color(&self.method))
            .child(self.method.as_str().to_string())
    }
}

#[derive(IntoElement)]
pub struct KbdChip {
    label: SharedString,
    theme: Theme,
}

impl KbdChip {
    pub fn new(label: impl Into<SharedString>, theme: Theme) -> Self {
        Self {
            label: label.into(),
            theme,
        }
    }
}

impl RenderOnce for KbdChip {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .font_family("monospace")
            .text_size(px(10.5))
            .text_color(self.theme.text_muted)
            .bg(self.theme.surface_raised)
            .border_1()
            .border_color(self.theme.border)
            .rounded(px(3.))
            .px(px(4.))
            .child(self.label)
    }
}

#[derive(IntoElement)]
pub struct TitlebarButton {
    label: SharedString,
    shortcut: SharedString,
    theme: Theme,
    on_click: Option<ClickHandler>,
}

impl TitlebarButton {
    pub fn new(
        label: impl Into<SharedString>,
        shortcut: impl Into<SharedString>,
        theme: Theme,
    ) -> Self {
        Self {
            label: label.into(),
            shortcut: shortcut.into(),
            theme,
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
        let theme = self.theme;
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
            .child(KbdChip::new(self.shortcut, theme).render(window, cx))
    }
}

/// One icon button in the left activity rail. Buttons without an `on_click`
/// render as inert (used for not-yet-implemented entries).
#[derive(IntoElement)]
pub struct RailButton {
    icon_name: IconName,
    tooltip: SharedString,
    active: bool,
    theme: Theme,
    on_click: Option<ClickHandler>,
}

impl RailButton {
    pub fn new(icon_name: IconName, tooltip: impl Into<SharedString>, theme: Theme) -> Self {
        Self {
            icon_name,
            tooltip: tooltip.into(),
            active: false,
            theme,
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
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
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
            .aria_label(self.tooltip)
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
