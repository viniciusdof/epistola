use std::ops::Range;

use gpui::{
    fill, point, px, relative, size, App, Bounds, Element, ElementId, ElementInputHandler, Entity,
    FocusHandle, GlobalElementId, InspectorElementId, IntoElement, LayoutId, PaintQuad, Pixels,
    Point, ShapedLine, Style, TextAlign, TextRun, Window,
};

use crate::components::editor::tokenize_line;
use crate::root::EpistolaGui;
use crate::state::ActiveFile;
use crate::theme::Theme;

pub(crate) struct LineGeometry {
    pub shaped: ShapedLine,
    pub byte_range: Range<usize>,
    pub y: Pixels,
}

pub(crate) struct EditorLayout {
    pub file: ActiveFile,
    pub bounds: Bounds<Pixels>,
    pub line_height: Pixels,
    pub lines: Vec<LineGeometry>,
}

impl EditorLayout {
    pub fn offset_for_point(&self, point: Point<Pixels>) -> usize {
        let Some(last) = self.lines.last() else {
            return 0;
        };
        let local_x = {
            let raw = point.x - self.bounds.left();
            if raw < px(0.) {
                px(0.)
            } else {
                raw
            }
        };
        for line in &self.lines {
            if point.y < line.y + self.line_height {
                return line.byte_range.start + line.shaped.closest_index_for_x(local_x);
            }
        }
        last.byte_range.start + last.shaped.closest_index_for_x(local_x)
    }

    pub fn bounds_for_range(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        let line = self.lines.iter().find(|line| {
            line.byte_range.start <= range.start && range.start <= line.byte_range.end
        })?;
        let local_start = range.start - line.byte_range.start;
        let local_end = range.end.min(line.byte_range.end) - line.byte_range.start;
        let x1 = line.shaped.x_for_index(local_start);
        let x2 = line.shaped.x_for_index(local_end);
        Some(Bounds::from_corners(
            point(self.bounds.left() + x1, line.y),
            point(self.bounds.left() + x2, line.y + self.line_height),
        ))
    }
}

pub(crate) struct EditorTextElement {
    pub gui: Entity<EpistolaGui>,
    pub theme: Theme,
    pub focus_handle: FocusHandle,
}

impl IntoElement for EditorTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

pub(crate) struct EditorPrepaintState {
    rows: Vec<LineGeometry>,
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
}

impl Element for EditorTextElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorPrepaintState;

    fn id(&self) -> Option<ElementId> {
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
        let line_count = self
            .gui
            .read(cx)
            .state
            .active_buffer()
            .map(|buffer| buffer.line_start_offsets().len())
            .unwrap_or(1)
            .max(1);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (window.line_height() * line_count as f32).into();
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
        let gui = self.gui.read(cx);
        let Some(buffer) = gui.state.active_buffer() else {
            return EditorPrepaintState {
                rows: Vec::new(),
                cursor: None,
                selections: Vec::new(),
            };
        };
        let text = buffer.text.clone();
        let selected_range = buffer.selected_range.clone();
        let cursor_offset = buffer.cursor_offset();
        let line_starts = buffer.line_start_offsets();

        let theme = self.theme;
        let text_style = window.text_style();
        let font = text_style.font();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();

        let mut rows = Vec::with_capacity(line_starts.len());
        let mut selections = Vec::new();
        let mut cursor = None;

        for (i, &start) in line_starts.iter().enumerate() {
            let end = line_starts
                .get(i + 1)
                .map(|&next| next - 1)
                .unwrap_or(text.len());
            let line_text = &text[start..end];
            let spans = tokenize_line(line_text);
            let runs: Vec<TextRun> = spans
                .iter()
                .map(|(kind, span)| TextRun {
                    len: span.len(),
                    font: font.clone(),
                    color: kind.color(theme),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                })
                .collect();
            let shaped = window.text_system().shape_line(
                line_text.to_string().into(),
                font_size,
                &runs,
                None,
            );

            let y = bounds.top() + line_height * i as f32;

            let sel_start = selected_range.start.clamp(start, end);
            let sel_end = selected_range.end.clamp(start, end);
            if sel_start < sel_end {
                let x1 = shaped.x_for_index(sel_start - start);
                let x2 = shaped.x_for_index(sel_end - start);
                selections.push(fill(
                    Bounds::from_corners(
                        point(bounds.left() + x1, y),
                        point(bounds.left() + x2, y + line_height),
                    ),
                    theme.accent.opacity(0.25),
                ));
            }

            if selected_range.is_empty() && cursor_offset >= start && cursor_offset <= end {
                let x = shaped.x_for_index(cursor_offset - start);
                cursor = Some(fill(
                    Bounds::new(point(bounds.left() + x, y), size(px(2.), line_height)),
                    theme.text,
                ));
            }

            rows.push(LineGeometry {
                shaped,
                byte_range: start..end,
                y,
            });
        }

        EditorPrepaintState {
            rows,
            cursor,
            selections,
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
        window.handle_input(
            &self.focus_handle,
            ElementInputHandler::new(bounds, self.gui.clone()),
            cx,
        );

        for selection in prepaint.selections.drain(..) {
            window.paint_quad(selection);
        }

        let line_height = window.line_height();
        for row in &prepaint.rows {
            let _ = row.shaped.paint(
                point(bounds.left(), row.y),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }

        if self.focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        let file = self.gui.read(cx).state.active_file.clone();
        let lines = std::mem::take(&mut prepaint.rows);
        self.gui.update(cx, |gui, _cx| {
            gui.editor_layout = Some(EditorLayout {
                file,
                bounds,
                line_height,
                lines,
            });
        });
    }
}
