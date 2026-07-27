use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    fill, point, px, size, App, AvailableSpace, Bounds, Element, ElementId, ElementInputHandler,
    Entity, FocusHandle, GlobalElementId, InspectorElementId, IntoElement, LayoutId, PaintQuad,
    Pixels, Point, ScrollHandle, SharedString, Style, TextAlign, TextRun, Window, WrappedLine,
};

use crate::components::editor::tokenize_for;
use crate::editor_view::EditorView;
use crate::state::ActiveFile;
use crate::theme::Theme;

struct EditorTextLayoutInner {
    lines: Vec<WrappedLine>,
    /// Absolute byte offset of the start of each logical line (parallel to `lines`).
    line_starts: Vec<usize>,
    line_height: Pixels,
    file: ActiveFile,
    bounds: Bounds<Pixels>,
    /// Absolute window Y of the top of each logical line's first visual row (parallel to `lines`).
    row_ys: Vec<Pixels>,
}

#[derive(Clone, Default)]
pub(crate) struct EditorTextLayout(Rc<RefCell<Option<EditorTextLayoutInner>>>);

impl EditorTextLayout {
    pub fn matches_file(&self, file: &ActiveFile) -> bool {
        self.0
            .borrow()
            .as_ref()
            .is_some_and(|inner| &inner.file == file)
    }

    pub fn offset_for_point(&self, position: Point<Pixels>) -> usize {
        let inner_ref = self.0.borrow();
        let Some(inner) = inner_ref.as_ref() else {
            return 0;
        };
        let Some(last_ix) = inner.lines.len().checked_sub(1) else {
            return 0;
        };
        let local_x = (position.x - inner.bounds.left()).max(px(0.));
        for (i, wrapped) in inner.lines.iter().enumerate() {
            let y = inner.row_ys[i];
            let height = wrapped.size(inner.line_height).height;
            if position.y < y + height || i == last_ix {
                // Clamp inside the line's own rows: a `local_y` at or past `height` would ask
                // gpui for a wrapped-row index past the last one it knows about, which it
                // reports as offset 0 rather than end-of-line.
                let mut local_y = (position.y - y).max(px(0.));
                if local_y >= height {
                    local_y = (height - inner.line_height).max(px(0.));
                }
                let local_index = wrapped
                    .closest_index_for_position(point(local_x, local_y), inner.line_height)
                    .unwrap_or_else(|ix| ix);
                return inner.line_starts[i] + local_index;
            }
        }
        0
    }

    pub fn bounds_for_range(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        let inner_ref = self.0.borrow();
        let inner = inner_ref.as_ref()?;
        if inner.line_starts.is_empty() {
            return None;
        }
        let i = inner
            .line_starts
            .partition_point(|&start| start <= range.start)
            .max(1)
            - 1;
        let wrapped = inner.lines.get(i)?;
        let line_start = inner.line_starts[i];
        let line_end = line_start + wrapped.len();
        let local_start = range.start.saturating_sub(line_start);
        let local_end = range.end.min(line_end).saturating_sub(line_start);
        let p1 = wrapped.position_for_index(local_start, inner.line_height)?;
        let p2 = wrapped.position_for_index(local_end, inner.line_height)?;
        let y = inner.row_ys[i];
        Some(Bounds::from_corners(
            point(inner.bounds.left() + p1.x, y + p1.y),
            point(inner.bounds.left() + p2.x, y + p2.y + inner.line_height),
        ))
    }
}

/// Local (0-based) `(start, end)` byte ranges of each visual row within a wrapped logical line.
fn visual_row_ranges(wrapped: &WrappedLine) -> Vec<(usize, usize)> {
    let runs = wrapped.runs();
    let mut bounds_ix: Vec<usize> = wrapped
        .wrap_boundaries()
        .iter()
        .map(|boundary| runs[boundary.run_ix].glyphs[boundary.glyph_ix].index)
        .collect();
    bounds_ix.push(wrapped.len());

    let mut ranges = Vec::with_capacity(bounds_ix.len());
    let mut start = 0;
    for end in bounds_ix {
        ranges.push((start, end));
        start = end;
    }
    ranges
}

pub(crate) struct EditorTextElement {
    pub editor_view: Entity<EditorView>,
    pub theme: Theme,
    pub focus_handle: FocusHandle,
    pub scroll_handle: ScrollHandle,
}

impl EditorTextElement {
    fn scroll_cursor_into_view(&self, cursor_y: Pixels, line_height: Pixels, window: &mut Window) {
        let scroll_handle = self.scroll_handle.clone();
        window.on_next_frame(move |_window, _cx| {
            let viewport = scroll_handle.bounds();
            if viewport.size.height <= px(0.) {
                return;
            }
            let offset = scroll_handle.offset();
            if cursor_y < viewport.top() {
                let delta = viewport.top() - cursor_y;
                scroll_handle.set_offset(point(offset.x, offset.y + delta));
            } else if cursor_y + line_height > viewport.bottom() {
                let delta = (cursor_y + line_height) - viewport.bottom();
                scroll_handle.set_offset(point(offset.x, offset.y - delta));
            }
        });
    }
}

impl IntoElement for EditorTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

pub(crate) struct EditorPrepaintState {
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
}

impl Element for EditorTextElement {
    type RequestLayoutState = EditorTextLayout;
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
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let layout = EditorTextLayout::default();
        let measure_layout = layout.clone();
        let editor_view = self.editor_view.clone();
        let theme = self.theme;

        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();

        let layout_id = window.request_measured_layout(style, {
            move |known_dimensions, available_space, window, cx| {
                let line_height = window.line_height();
                let wrap_width = known_dimensions.width.or(match available_space.width {
                    AvailableSpace::Definite(width) => Some(width),
                    _ => None,
                });

                let view_ref = editor_view.read(cx);
                let Some(buffer) = view_ref.active_buffer() else {
                    measure_layout
                        .0
                        .borrow_mut()
                        .replace(EditorTextLayoutInner {
                            lines: Vec::new(),
                            line_starts: vec![0],
                            line_height,
                            file: ActiveFile::None,
                            bounds: Bounds::default(),
                            row_ys: Vec::new(),
                        });
                    return size(px(0.), line_height);
                };
                let text = buffer.text.clone();
                let content_kind = buffer.content_kind;
                let line_starts = buffer.line_start_offsets().to_vec();
                let file = view_ref.active_file().clone();

                let text_style = window.text_style();
                let font = text_style.font();
                let font_size = text_style.font_size.to_pixels(window.rem_size());

                let mut runs = Vec::new();
                for (i, &start) in line_starts.iter().enumerate() {
                    let end = line_starts
                        .get(i + 1)
                        .map(|&next| next - 1)
                        .unwrap_or(text.len());
                    for (kind, len) in tokenize_for(content_kind, &text[start..end]) {
                        if len > 0 {
                            runs.push(TextRun {
                                len,
                                font: font.clone(),
                                color: kind.color(theme),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            });
                        }
                    }
                    if i + 1 < line_starts.len() {
                        runs.push(TextRun {
                            len: 1, // the '\n' separator
                            font: font.clone(),
                            color: theme.text,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        });
                    }
                }

                let lines: Vec<WrappedLine> = window
                    .text_system()
                    .shape_text(SharedString::from(text), font_size, &runs, wrap_width, None)
                    .ok()
                    .map(|lines| lines.into_iter().collect())
                    .unwrap_or_default();

                let mut total = size(px(0.), px(0.));
                for line in &lines {
                    let line_size = line.size(line_height);
                    total.height += line_size.height;
                    total.width = total.width.max(line_size.width);
                }

                measure_layout
                    .0
                    .borrow_mut()
                    .replace(EditorTextLayoutInner {
                        lines,
                        line_starts,
                        line_height,
                        file,
                        bounds: Bounds::default(),
                        row_ys: Vec::new(),
                    });

                total
            }
        });

        (layout_id, layout)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let line_height = {
            let mut inner_ref = request_layout.0.borrow_mut();
            let Some(inner) = inner_ref.as_mut() else {
                return EditorPrepaintState {
                    cursor: None,
                    selections: Vec::new(),
                };
            };
            inner.bounds = bounds;
            let mut row_ys = Vec::with_capacity(inner.lines.len());
            let mut y = bounds.top();
            for line in inner.lines.iter() {
                row_ys.push(y);
                y += line.size(inner.line_height).height;
            }
            inner.row_ys = row_ys;
            inner.line_height
        };

        let editor_view = self.editor_view.read(cx);
        let Some(buffer) = editor_view.active_buffer() else {
            return EditorPrepaintState {
                cursor: None,
                selections: Vec::new(),
            };
        };
        let selected_range = buffer.selected_range.clone();
        let cursor_offset = buffer.cursor_offset();

        let theme = self.theme;
        let inner_ref = request_layout.0.borrow();
        let Some(inner) = inner_ref.as_ref() else {
            return EditorPrepaintState {
                cursor: None,
                selections: Vec::new(),
            };
        };

        let mut selections = Vec::new();
        let mut cursor = None;

        for (i, wrapped) in inner.lines.iter().enumerate() {
            let start = inner.line_starts[i];
            let end = start + wrapped.len();
            let y = inner.row_ys[i];

            let sel_start = selected_range.start.clamp(start, end);
            let sel_end = selected_range.end.clamp(start, end);
            if sel_start < sel_end {
                for (row_start, row_end) in visual_row_ranges(wrapped) {
                    let clamp_start = (sel_start - start).max(row_start);
                    let clamp_end = (sel_end - start).min(row_end);
                    if clamp_start < clamp_end {
                        if let (Some(p1), Some(p2)) = (
                            wrapped.position_for_index(clamp_start, line_height),
                            wrapped.position_for_index(clamp_end, line_height),
                        ) {
                            selections.push(fill(
                                Bounds::from_corners(
                                    point(inner.bounds.left() + p1.x, y + p1.y),
                                    point(inner.bounds.left() + p2.x, y + p2.y + line_height),
                                ),
                                theme.accent.opacity(0.25),
                            ));
                        }
                    }
                }
            }

            if selected_range.is_empty() && cursor_offset >= start && cursor_offset <= end {
                let local = cursor_offset - start;
                if let Some(p) = wrapped.position_for_index(local, line_height) {
                    cursor = Some(fill(
                        Bounds::new(
                            point(inner.bounds.left() + p.x, y + p.y),
                            size(px(2.), line_height),
                        ),
                        theme.text,
                    ));
                }
            }
        }

        EditorPrepaintState { cursor, selections }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.handle_input(
            &self.focus_handle,
            ElementInputHandler::new(bounds, self.editor_view.clone()),
            cx,
        );

        for selection in prepaint.selections.drain(..) {
            window.paint_quad(selection);
        }

        let line_height = {
            let inner_ref = request_layout.0.borrow();
            let Some(inner) = inner_ref.as_ref() else {
                return;
            };
            for (i, wrapped_line) in inner.lines.iter().enumerate() {
                let y = inner.row_ys[i];
                let _ = wrapped_line.paint(
                    point(bounds.left(), y),
                    inner.line_height,
                    TextAlign::Left,
                    Some(bounds),
                    window,
                    cx,
                );
            }
            inner.line_height
        };

        if self.focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                let cursor_y = cursor.bounds.top();
                window.paint_quad(cursor);
                self.scroll_cursor_into_view(cursor_y, line_height, window);
            }
        }

        self.editor_view.update(cx, |editor_view, _cx| {
            editor_view.layout = Some(request_layout.clone());
        });
    }
}
