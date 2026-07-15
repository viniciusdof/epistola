use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};

use crate::root::EpistolaGui;

impl EntityInputHandler for EpistolaGui {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let buffer = self.state.active_buffer()?;
        let range = buffer.range_from_utf16(&range_utf16);
        actual_range.replace(buffer.range_to_utf16(&range));
        Some(buffer.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let buffer = self.state.active_buffer()?;
        Some(UTF16Selection {
            range: buffer.range_to_utf16(&buffer.selected_range),
            reversed: buffer.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let buffer = self.state.active_buffer()?;
        buffer
            .marked_range
            .as_ref()
            .map(|range| buffer.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(buffer) = self.state.active_buffer_mut() {
            buffer.marked_range = None;
            buffer.cancel_composition();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(buffer) = self.state.active_buffer_mut() else {
            return;
        };
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| buffer.range_from_utf16(range_utf16))
            .or_else(|| buffer.marked_range.clone())
            .unwrap_or_else(|| buffer.selected_range.clone());
        buffer.replace_range(range, new_text);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(buffer) = self.state.active_buffer_mut() else {
            return;
        };
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| buffer.range_from_utf16(range_utf16))
            .or_else(|| buffer.marked_range.clone())
            .unwrap_or_else(|| buffer.selected_range.clone());

        buffer.set_marked_text(range.clone(), new_text);
        buffer.marked_range = if new_text.is_empty() {
            None
        } else {
            Some(range.start..range.start + new_text.len())
        };
        buffer.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| buffer.range_from_utf16(range_utf16))
            .map(|new_range| range.start + new_range.start..range.start + new_range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let buffer = self.state.active_buffer()?;
        let range = buffer.range_from_utf16(&range_utf16);
        let layout = self.editor_layout.as_ref()?;
        if !layout.matches_file(&self.state.active_file) {
            return None;
        }
        layout.bounds_for_range(range)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let buffer = self.state.active_buffer()?;
        let layout = self.editor_layout.as_ref()?;
        if !layout.matches_file(&self.state.active_file) {
            return None;
        }
        Some(buffer.offset_to_utf16(layout.offset_for_point(point)))
    }
}
