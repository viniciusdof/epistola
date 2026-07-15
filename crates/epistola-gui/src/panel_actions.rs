use std::path::PathBuf;

use gpui::{Context, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Window};

use crate::components::resize_handle::{clamp_panel_size, HANDLE_SIZE};
use crate::components::{activity_rail, statusbar, titlebar};
use crate::root::{EpistolaGui, ResizingPanel};

const MIN_EDITOR_WIDTH: gpui::Pixels = gpui::px(240.);

const MIN_CONTENT_HEIGHT: gpui::Pixels = gpui::px(160.);

impl EpistolaGui {
    pub(crate) fn start_sidebar_resize(
        &mut self,
        _event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.resizing = Some(ResizingPanel::Sidebar);
        cx.notify();
    }

    pub(crate) fn start_drawer_resize(
        &mut self,
        _event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.resizing = Some(ResizingPanel::Drawer);
        cx.notify();
    }

    pub(crate) fn on_root_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(panel) = self.resizing else {
            return;
        };
        let viewport = window.viewport_size();
        match panel {
            ResizingPanel::Sidebar => {
                let candidate = event.position.x - activity_rail::RAIL_WIDTH;
                let reserved = activity_rail::RAIL_WIDTH + HANDLE_SIZE + MIN_EDITOR_WIDTH;
                self.state.sidebar_width =
                    clamp_panel_size(candidate, gpui::px(0.), viewport.width, reserved);
            }
            ResizingPanel::Drawer => {
                let candidate = (viewport.height - statusbar::STATUSBAR_HEIGHT)
                    - event.position.y
                    - HANDLE_SIZE;
                let reserved = titlebar::TITLEBAR_HEIGHT
                    + statusbar::STATUSBAR_HEIGHT
                    + HANDLE_SIZE
                    + MIN_CONTENT_HEIGHT;
                self.state.drawer_height =
                    clamp_panel_size(candidate, gpui::px(0.), viewport.height, reserved);
            }
        }
        cx.notify();
    }

    pub(crate) fn stop_resizing(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.resizing.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.state.sidebar_collapsed = !self.state.sidebar_collapsed;
        cx.notify();
    }

    pub(crate) fn toggle_drawer(&mut self, cx: &mut Context<Self>) {
        self.state.drawer_collapsed = !self.state.drawer_collapsed;
        cx.notify();
    }

    pub(crate) fn toggle_folder_collapse(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        self.state.toggle_folder_collapse(dir);
        cx.notify();
    }
}
