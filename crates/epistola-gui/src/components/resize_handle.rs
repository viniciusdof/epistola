use gpui::{div, prelude::*, px, Div, Pixels, SharedString, Stateful};

use crate::theme::Theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResizeAxis {
    Horizontal,
    Vertical,
}

pub const HANDLE_SIZE: Pixels = px(10.);

const LINE_THICKNESS: Pixels = px(1.);

pub fn resize_handle(id: impl Into<SharedString>, axis: ResizeAxis, theme: Theme) -> Stateful<Div> {
    let group = "resize-handle";
    let line = div()
        .group_hover(group, |el| el.bg(theme.accent))
        .bg(theme.border);
    let line = match axis {
        ResizeAxis::Horizontal => line.w(LINE_THICKNESS).h_full(),
        ResizeAxis::Vertical => line.h(LINE_THICKNESS).w_full(),
    };

    let handle = div()
        .id(id.into())
        .group(group)
        .flex_none()
        .flex()
        .items_center()
        .justify_center();
    let handle = match axis {
        ResizeAxis::Horizontal => handle.w(HANDLE_SIZE).h_full().cursor_ew_resize(),
        ResizeAxis::Vertical => handle.h(HANDLE_SIZE).w_full().cursor_ns_resize(),
    };
    handle.child(line)
}

pub fn clamp_panel_size(
    candidate: Pixels,
    min: Pixels,
    viewport: Pixels,
    reserved: Pixels,
) -> Pixels {
    let max = (viewport - reserved).max(min);
    candidate.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_panel_size_respects_minimum() {
        assert_eq!(
            clamp_panel_size(px(50.), px(160.), px(1000.), px(300.)),
            px(160.)
        );
    }

    #[test]
    fn clamp_panel_size_respects_maximum() {
        assert_eq!(
            clamp_panel_size(px(900.), px(160.), px(1000.), px(300.)),
            px(700.)
        );
    }

    #[test]
    fn clamp_panel_size_passes_through_within_bounds() {
        assert_eq!(
            clamp_panel_size(px(400.), px(160.), px(1000.), px(300.)),
            px(400.)
        );
    }

    #[test]
    fn clamp_panel_size_falls_back_to_min_on_narrow_viewport() {
        assert_eq!(
            clamp_panel_size(px(900.), px(160.), px(200.), px(300.)),
            px(160.)
        );
    }
}
