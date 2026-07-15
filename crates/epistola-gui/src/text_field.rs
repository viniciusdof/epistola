use std::ops::Range;

use gpui::{
    div, fill, point, prelude::*, px, relative, size, App, Bounds, ClipboardItem, Context,
    CursorStyle, Element, ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style,
    TextRun, UTF16Selection, UnderlineStyle, Window,
};

use crate::actions::{
    Backspace, Copy, Cut, Delete, End, Home, MoveLeft, MoveRight, Paste, SelectAll, SelectLeft,
    SelectRight,
};
use crate::buffer::EditorBuffer;
use crate::theme::Theme;

pub struct TextField {
    buffer: EditorBuffer,
    focus_handle: FocusHandle,
    placeholder: SharedString,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextField {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut App) -> Self {
        Self {
            buffer: EditorBuffer::new(String::new()),
            focus_handle: cx.focus_handle(),
            placeholder: placeholder.into(),
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    pub fn text(&self) -> &str {
        &self.buffer.text
    }

    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let text = text.into();
        self.buffer.move_to(text.len());
        self.buffer.text = text;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_text(String::new(), cx);
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        self.buffer.replace_active_range(&text.replace('\n', ""));
        cx.notify();
    }

    fn backspace(&mut self, _: &Backspace, _window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.selected_range.is_empty() {
            let prev = self.buffer.previous_boundary(self.buffer.cursor_offset());
            if prev == self.buffer.cursor_offset() {
                return;
            }
            self.buffer.select_to(prev);
        }
        self.insert("", cx);
    }

    fn delete(&mut self, _: &Delete, _window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.selected_range.is_empty() {
            let next = self.buffer.next_boundary(self.buffer.cursor_offset());
            if next == self.buffer.cursor_offset() {
                return;
            }
            self.buffer.select_to(next);
        }
        self.insert("", cx);
    }

    fn move_left(&mut self, _: &MoveLeft, _window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.selected_range.is_empty() {
            let prev = self.buffer.previous_boundary(self.buffer.cursor_offset());
            self.buffer.move_to(prev);
        } else {
            self.buffer.move_to(self.buffer.selected_range.start);
        }
        cx.notify();
    }

    fn move_right(&mut self, _: &MoveRight, _window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.selected_range.is_empty() {
            let next = self.buffer.next_boundary(self.buffer.cursor_offset());
            self.buffer.move_to(next);
        } else {
            self.buffer.move_to(self.buffer.selected_range.end);
        }
        cx.notify();
    }

    fn select_left(&mut self, _: &SelectLeft, _window: &mut Window, cx: &mut Context<Self>) {
        let prev = self.buffer.previous_boundary(self.buffer.cursor_offset());
        self.buffer.select_to(prev);
        cx.notify();
    }

    fn select_right(&mut self, _: &SelectRight, _window: &mut Window, cx: &mut Context<Self>) {
        let next = self.buffer.next_boundary(self.buffer.cursor_offset());
        self.buffer.select_to(next);
        cx.notify();
    }

    fn select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_to(0);
        self.buffer.select_to(self.buffer.text.len());
        cx.notify();
    }

    fn home(&mut self, _: &Home, _window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_to(0);
        cx.notify();
    }

    fn end(&mut self, _: &End, _window: &mut Window, cx: &mut Context<Self>) {
        let len = self.buffer.text.len();
        self.buffer.move_to(len);
        cx.notify();
    }

    fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buffer.text[self.buffer.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.selected_range.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.buffer.text[self.buffer.selected_range.clone()].to_string(),
        ));
        self.insert("", cx);
    }

    fn paste(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.insert(&text, cx);
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.buffer.text.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.buffer.text.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.is_selecting = true;
        let offset = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.buffer.select_to(offset);
        } else {
            self.buffer.move_to(offset);
        }
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting {
            return;
        }
        let offset = self.index_for_mouse_position(event.position);
        self.buffer.select_to(offset);
        cx.notify();
    }
}

impl Focusable for TextField {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.buffer.range_from_utf16(&range_utf16);
        actual_range.replace(self.buffer.range_to_utf16(&range));
        Some(self.buffer.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.buffer.range_to_utf16(&self.buffer.selected_range),
            reversed: self.buffer.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.buffer
            .marked_range
            .as_ref()
            .map(|range| self.buffer.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.buffer.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.buffer.range_from_utf16(range_utf16))
            .or_else(|| self.buffer.marked_range.clone())
            .unwrap_or_else(|| self.buffer.selected_range.clone());
        self.buffer
            .replace_range(range, &new_text.replace('\n', ""));
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
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.buffer.range_from_utf16(range_utf16))
            .or_else(|| self.buffer.marked_range.clone())
            .unwrap_or_else(|| self.buffer.selected_range.clone());
        let new_text = new_text.replace('\n', "");

        self.buffer.text.replace_range(range.clone(), &new_text);
        self.buffer.marked_range = if new_text.is_empty() {
            None
        } else {
            Some(range.start..range.start + new_text.len())
        };
        self.buffer.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.buffer.range_from_utf16(range_utf16))
            .map(|new_range| range.start + new_range.start..range.start + new_range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.buffer.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let local = bounds.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(local.x)?;
        Some(self.buffer.offset_to_utf16(utf8_index))
    }
}

struct TextFieldElement {
    field: Entity<TextField>,
    theme: Theme,
}

struct TextFieldPrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextFieldElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextFieldElement {
    type RequestLayoutState = ();
    type PrepaintState = TextFieldPrepaintState;

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let field = self.field.read(cx);
        let text = field.buffer.text.clone();
        let selected_range = field.buffer.selected_range.clone();
        let cursor_offset = field.buffer.cursor_offset();
        let marked_range = field.buffer.marked_range.clone();

        let theme = self.theme;
        let text_style = window.text_style();
        let font = text_style.font();
        let font_size = text_style.font_size.to_pixels(window.rem_size());

        let (display_text, base_color): (SharedString, _) = if text.is_empty() {
            (field.placeholder.clone(), theme.text_faint)
        } else {
            (text.clone().into(), theme.text)
        };

        let base_run = TextRun {
            len: display_text.len(),
            font,
            color: base_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match &marked_range {
            Some(marked_range) if !text.is_empty() => vec![
                TextRun {
                    len: marked_range.start,
                    ..base_run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(base_run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..base_run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..base_run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect(),
            _ => vec![base_run],
        };

        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let (selection, cursor) = if selected_range.is_empty() {
            let x = line.x_for_index(cursor_offset);
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + x, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    theme.text,
                )),
            )
        } else {
            let x1 = line.x_for_index(selected_range.start);
            let x2 = line.x_for_index(selected_range.end);
            (
                Some(fill(
                    Bounds::from_corners(
                        point(bounds.left() + x1, bounds.top()),
                        point(bounds.left() + x2, bounds.bottom()),
                    ),
                    theme.accent.opacity(0.25),
                )),
                None,
            )
        };

        TextFieldPrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.field.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.field.clone()),
            cx,
        );

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }

        let line = prepaint.line.take().unwrap_or_else(|| {
            window
                .text_system()
                .shape_line("".into(), px(13.), &[], None)
        });
        let _ = line.paint(
            bounds.origin,
            window.line_height(),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        self.field.update(cx, |field, _cx| {
            field.last_layout = Some(line);
            field.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextField {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        div()
            .key_context("TextField")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(TextFieldElement {
                field: cx.entity(),
                theme,
            })
    }
}
