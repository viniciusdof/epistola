use gpui::{
    ClipboardItem, Context, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Window,
};

use crate::actions::{
    Backspace, Copy, Cut, Delete, End, Home, InsertNewline, MoveDown, MoveLeft, MoveRight, MoveUp,
    Paste, Redo, Save, SelectAll, SelectDown, SelectLeft, SelectRight, SelectUp, Undo,
};
use crate::buffer::EditorBuffer;
use crate::editor_save;
use crate::root::EpistolaGui;

impl EpistolaGui {
    fn with_active_buffer(&mut self, f: impl FnOnce(&mut EditorBuffer)) {
        if let Some(buffer) = self.state.active_buffer_mut() {
            f(buffer);
        }
    }

    fn with_active_buffer_if_editable(&mut self, f: impl FnOnce(&mut EditorBuffer)) {
        self.with_active_buffer(|buffer| {
            if !buffer.read_only {
                f(buffer);
            }
        });
    }

    pub(crate) fn move_left(&mut self, _: &MoveLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer(|buffer| {
            buffer.desired_col = None;
            if buffer.selected_range.is_empty() {
                let prev = buffer.previous_boundary(buffer.cursor_offset());
                buffer.move_to(prev);
            } else {
                buffer.move_to(buffer.selected_range.start);
            }
        });
        cx.notify();
    }

    pub(crate) fn move_right(
        &mut self,
        _: &MoveRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_active_buffer(|buffer| {
            buffer.desired_col = None;
            if buffer.selected_range.is_empty() {
                let next = buffer.next_boundary(buffer.cursor_offset());
                buffer.move_to(next);
            } else {
                buffer.move_to(buffer.selected_range.end);
            }
        });
        cx.notify();
    }

    pub(crate) fn move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer(|buffer| {
            let offset = buffer.offset_vertical(-1);
            buffer.move_to(offset);
        });
        cx.notify();
    }

    pub(crate) fn move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer(|buffer| {
            let offset = buffer.offset_vertical(1);
            buffer.move_to(offset);
        });
        cx.notify();
    }

    pub(crate) fn select_left(
        &mut self,
        _: &SelectLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_active_buffer(|buffer| {
            buffer.desired_col = None;
            let prev = buffer.previous_boundary(buffer.cursor_offset());
            buffer.select_to(prev);
        });
        cx.notify();
    }

    pub(crate) fn select_right(
        &mut self,
        _: &SelectRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_active_buffer(|buffer| {
            buffer.desired_col = None;
            let next = buffer.next_boundary(buffer.cursor_offset());
            buffer.select_to(next);
        });
        cx.notify();
    }

    pub(crate) fn select_up(&mut self, _: &SelectUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer(|buffer| {
            let offset = buffer.offset_vertical(-1);
            buffer.select_to(offset);
        });
        cx.notify();
    }

    pub(crate) fn select_down(
        &mut self,
        _: &SelectDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_active_buffer(|buffer| {
            let offset = buffer.offset_vertical(1);
            buffer.select_to(offset);
        });
        cx.notify();
    }

    pub(crate) fn select_all(
        &mut self,
        _: &SelectAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_active_buffer(|buffer| {
            buffer.move_to(0);
            buffer.select_to(buffer.text.len());
        });
        cx.notify();
    }

    pub(crate) fn home(&mut self, _: &Home, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer(|buffer| {
            buffer.desired_col = None;
            let (line, _) = buffer.line_col_for_offset(buffer.cursor_offset());
            let offset = buffer.offset_for_line_col(line, 0);
            buffer.move_to(offset);
        });
        cx.notify();
    }

    pub(crate) fn end(&mut self, _: &End, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer(|buffer| {
            buffer.desired_col = None;
            let (line, _) = buffer.line_col_for_offset(buffer.cursor_offset());
            let offset = buffer.offset_for_line_col(line, usize::MAX);
            buffer.move_to(offset);
        });
        cx.notify();
    }

    pub(crate) fn undo(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer_if_editable(|buffer| buffer.undo());
        cx.notify();
    }

    pub(crate) fn redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer_if_editable(|buffer| buffer.redo());
        cx.notify();
    }

    pub(crate) fn backspace(
        &mut self,
        _: &Backspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_active_buffer_if_editable(|buffer| {
            if buffer.selected_range.is_empty() {
                let prev = buffer.previous_boundary(buffer.cursor_offset());
                if prev == buffer.cursor_offset() {
                    return;
                }
                buffer.select_to(prev);
            }
            buffer.replace_active_range("");
        });
        cx.notify();
    }

    pub(crate) fn insert_newline(
        &mut self,
        _: &InsertNewline,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_active_buffer_if_editable(|buffer| buffer.replace_active_range("\n"));
        cx.notify();
    }

    pub(crate) fn delete(&mut self, _: &Delete, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer_if_editable(|buffer| {
            if buffer.selected_range.is_empty() {
                let next = buffer.next_boundary(buffer.cursor_offset());
                if next == buffer.cursor_offset() {
                    return;
                }
                buffer.select_to(next);
            }
            buffer.replace_active_range("");
        });
        cx.notify();
    }

    fn selected_text(&self) -> Option<String> {
        let buffer = self.state.active_buffer()?;
        if buffer.selected_range.is_empty() {
            return None;
        }
        Some(buffer.text[buffer.selected_range.clone()].to_string())
    }

    pub(crate) fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    pub(crate) fn cut(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.selected_text() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.with_active_buffer_if_editable(|buffer| buffer.replace_active_range(""));
        cx.notify();
    }

    pub(crate) fn paste(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.with_active_buffer_if_editable(|buffer| buffer.replace_active_range(&text));
        cx.notify();
    }

    pub(crate) fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        let file = self.state.active_file.clone();
        editor_save::validate_and_save(&mut self.state, &file);
        cx.notify();
    }

    fn offset_for_point(&self, position: Point<Pixels>) -> Option<usize> {
        let layout = self.editor_layout.as_ref()?;
        if !layout.matches_file(&self.state.active_file) {
            return None;
        }
        Some(layout.offset_for_point(position))
    }

    pub(crate) fn on_editor_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.editor_focus_handle, cx);
        let Some(offset) = self.offset_for_point(event.position) else {
            return;
        };
        self.editor_mouse_selecting = true;
        let shift = event.modifiers.shift;
        self.with_active_buffer(|buffer| {
            buffer.desired_col = None;
            if shift {
                buffer.select_to(offset);
            } else {
                buffer.move_to(offset);
            }
        });
        cx.notify();
    }

    pub(crate) fn on_editor_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editor_mouse_selecting {
            return;
        }
        let Some(offset) = self.offset_for_point(event.position) else {
            return;
        };
        self.with_active_buffer(|buffer| buffer.select_to(offset));
        cx.notify();
    }

    pub(crate) fn on_editor_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.editor_mouse_selecting = false;
    }
}
