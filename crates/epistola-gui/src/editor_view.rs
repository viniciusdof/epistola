use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use epistola_format::{FolderManifest, RequestFile};
use gpui::{
    div, prelude::*, px, App, Bounds, ClipboardItem, Context, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render,
    ScrollHandle, UTF16Selection, Window,
};

use crate::actions::{
    Backspace, Copy, Cut, Delete, End, Home, InsertNewline, MoveDown, MoveLeft, MoveRight, MoveUp,
    Paste, Redo, Save, SelectAll, SelectDown, SelectLeft, SelectRight, SelectUp, Undo,
};
use crate::buffer::{ContentKind, EditorBuffer};
use crate::components::editor_text::{EditorTextElement, EditorTextLayout};
use crate::state::ActiveFile;
use crate::theme::Theme;

/// Emitted after a successful disk write so `EpistolaGui` can refresh the
/// active request's URL preview without `EditorView` needing to know about
/// `AppState`/`CollectionTree`.
pub(crate) struct EditorSaved {
    pub file: ActiveFile,
}

/// Chrome-facing snapshot computed once per render — lets `components::editor`
/// and `components::tab_strip` read save/dirty state without holding a live
/// borrow of the `EditorView` entity across the render tree they build.
pub(crate) struct EditorSnapshot {
    pub save_error: Option<String>,
    pub external_change: bool,
    pub dirty_tabs: HashSet<ActiveFile>,
}

pub struct EditorView {
    buffers: HashMap<ActiveFile, EditorBuffer>,
    active_file: ActiveFile,
    collection_root: Option<PathBuf>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) layout: Option<EditorTextLayout>,
    mouse_selecting: bool,
    scroll_handle: ScrollHandle,
}

impl EventEmitter<EditorSaved> for EditorView {}

impl EditorView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            buffers: HashMap::new(),
            active_file: ActiveFile::None,
            collection_root: None,
            focus_handle: cx.focus_handle(),
            layout: None,
            mouse_selecting: false,
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub(crate) fn snapshot(&self, open_tabs: &[ActiveFile]) -> EditorSnapshot {
        let buffer = self.active_buffer();
        EditorSnapshot {
            save_error: buffer.and_then(|b| b.save_error.clone()),
            external_change: buffer.is_some_and(|b| b.external_change),
            dirty_tabs: open_tabs
                .iter()
                .filter(|file| self.is_dirty(file))
                .cloned()
                .collect(),
        }
    }

    pub(crate) fn set_active_file(&mut self, file: ActiveFile) {
        self.active_file = file;
    }

    pub(crate) fn set_collection_root(&mut self, root: Option<PathBuf>) {
        self.collection_root = root;
    }

    pub(crate) fn active_file(&self) -> &ActiveFile {
        &self.active_file
    }

    pub(crate) fn has_buffer(&self, file: &ActiveFile) -> bool {
        self.buffers.contains_key(file)
    }

    pub(crate) fn active_buffer(&self) -> Option<&EditorBuffer> {
        self.buffers.get(&self.active_file)
    }

    pub(crate) fn active_buffer_mut(&mut self) -> Option<&mut EditorBuffer> {
        self.buffers.get_mut(&self.active_file)
    }

    pub(crate) fn is_dirty(&self, file: &ActiveFile) -> bool {
        self.buffers.get(file).is_some_and(EditorBuffer::is_dirty)
    }

    pub(crate) fn has_unsaved_changes(&self) -> bool {
        self.buffers.values().any(EditorBuffer::is_dirty)
    }

    /// Inserts a buffer for `file` if one isn't already open — a no-op
    /// otherwise, so re-opening an already-open tab never discards edits.
    pub(crate) fn ensure_buffer(&mut self, file: ActiveFile, text: String, read_only: bool) {
        self.buffers.entry(file).or_insert_with(|| {
            if read_only {
                EditorBuffer::read_only(text)
            } else {
                EditorBuffer::new(text)
            }
        });
    }

    /// Overwrites (or creates) a read-only buffer with fresh derived
    /// content — unlike `ensure_buffer`, always reflects the latest
    /// `text`/`content_kind` rather than a no-op once a buffer exists.
    /// Skips the write (and the selection/undo reset `set_text` implies)
    /// when nothing actually changed.
    pub(crate) fn set_readonly_text(
        &mut self,
        file: ActiveFile,
        text: String,
        content_kind: ContentKind,
    ) {
        match self.buffers.get_mut(&file) {
            Some(buffer) if buffer.text != text || buffer.content_kind != content_kind => {
                buffer.set_text(text);
                buffer.content_kind = content_kind;
            }
            Some(_) => {}
            None => {
                let mut buffer = EditorBuffer::read_only(text);
                buffer.content_kind = content_kind;
                self.buffers.insert(file, buffer);
            }
        }
    }

    pub(crate) fn remove_buffer(&mut self, file: &ActiveFile) {
        self.buffers.remove(file);
    }

    /// Moves the buffer at `old` (if any) to `new`, preserving its text and
    /// undo history — used on rename, where re-reading from disk would lose
    /// any unsaved edits made before the rename was confirmed.
    pub(crate) fn rename_buffer(&mut self, old: &ActiveFile, new: ActiveFile) {
        if let Some(buffer) = self.buffers.remove(old) {
            self.buffers.insert(new, buffer);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.buffers.clear();
        self.active_file = ActiveFile::None;
        self.collection_root = None;
    }

    /// Reacts to a debounced batch of externally-touched paths under the open
    /// collection. Returns the `Request` paths that were reloaded so the
    /// caller can refresh their URL preview.
    pub(crate) fn handle_fs_events(
        &mut self,
        paths: &[PathBuf],
        collection_root: Option<&Path>,
    ) -> Vec<PathBuf> {
        reload_touched_buffers(&mut self.buffers, paths, collection_root)
    }

    /// Validates and writes `file`'s buffer verbatim (never through a typed
    /// serializer, which would reformat the file and drop hand-written
    /// comments). A no-op if `file` has no buffer or is read-only.
    pub(crate) fn save_file(&mut self, file: &ActiveFile, cx: &mut Context<Self>) {
        if save_buffer_to_disk(&mut self.buffers, file, self.collection_root.as_deref()) {
            cx.emit(EditorSaved { file: file.clone() });
        }
    }

    fn with_active_buffer(&mut self, f: impl FnOnce(&mut EditorBuffer)) {
        if let Some(buffer) = self.active_buffer_mut() {
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
        let buffer = self.active_buffer()?;
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

    pub(crate) fn save_active(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        let file = self.active_file.clone();
        self.save_file(&file, cx);
    }

    fn offset_for_point(&self, position: Point<Pixels>) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        if !layout.matches_file(&self.active_file) {
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
        window.focus(&self.focus_handle, cx);
        let Some(offset) = self.offset_for_point(event.position) else {
            return;
        };
        self.mouse_selecting = true;
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
        if !self.mouse_selecting {
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
        self.mouse_selecting = false;
    }
}

/// Validates and writes `file`'s buffer verbatim. Returns whether it was
/// written successfully, so callers can decide whether to signal "saved".
/// Kept as a free function over plain `buffers` (rather than a method on
/// `EditorView`) so it's testable without a `FocusHandle`/GPUI context.
fn save_buffer_to_disk(
    buffers: &mut HashMap<ActiveFile, EditorBuffer>,
    file: &ActiveFile,
    collection_root: Option<&Path>,
) -> bool {
    let Some(buffer) = buffers.get(file) else {
        return false;
    };
    if buffer.read_only {
        return false;
    }
    let text = buffer.text.clone();

    let validation: Result<(), String> = match file {
        ActiveFile::Request(_) => RequestFile::from_toml_str(&text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        ActiveFile::Folder(_) => FolderManifest::from_toml_str(&text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        ActiveFile::Environment(_) => {
            epistola_format::validate_variables_toml(&text).map_err(|e| e.to_string())
        }
        ActiveFile::Config | ActiveFile::None => Err("this tab can't be saved".to_string()),
    };

    if let Err(message) = validation {
        if let Some(buffer) = buffers.get_mut(file) {
            buffer.save_error = Some(message);
        }
        return false;
    }

    let Some(path) = file.disk_path(collection_root) else {
        return false;
    };
    let result = std::fs::write(&path, &text);
    let Some(buffer) = buffers.get_mut(file) else {
        return false;
    };
    match result {
        Ok(()) => {
            buffer.mark_saved();
            true
        }
        Err(err) => {
            buffer.save_error = Some(err.to_string());
            false
        }
    }
}

/// Reloads any open buffer whose disk file is in `paths` and isn't dirty;
/// dirty buffers are flagged `external_change` instead of overwritten.
/// Returns the `Request` paths that were actually reloaded. Free function
/// for the same testability reason as `save_buffer_to_disk`.
fn reload_touched_buffers(
    buffers: &mut HashMap<ActiveFile, EditorBuffer>,
    paths: &[PathBuf],
    collection_root: Option<&Path>,
) -> Vec<PathBuf> {
    let touched: HashSet<PathBuf> = paths.iter().cloned().collect();
    let files: Vec<ActiveFile> = buffers.keys().cloned().collect();
    let mut reloaded = Vec::new();
    for file in files {
        let Some(disk_path) = file.disk_path(collection_root) else {
            continue;
        };
        if !touched.contains(&disk_path) {
            continue;
        }
        let is_dirty = buffers.get(&file).is_some_and(EditorBuffer::is_dirty);
        if is_dirty {
            if let Some(buffer) = buffers.get_mut(&file) {
                buffer.external_change = true;
            }
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&disk_path) else {
            continue;
        };
        let Some(buffer) = buffers.get(&file) else {
            continue;
        };
        // Same text is either the app's own write echoing back through the
        // watcher, or a no-op external touch — reloading would wipe undo
        // history for nothing.
        if buffer.text == text {
            continue;
        }
        if let Some(buffer) = buffers.get_mut(&file) {
            buffer.set_text(text);
            buffer.mark_saved();
        }
        if let ActiveFile::Request(path) = &file {
            reloaded.push(path.clone());
        }
    }
    reloaded
}

impl Focusable for EditorView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let buffer = self.active_buffer()?;
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
        let buffer = self.active_buffer()?;
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
        let buffer = self.active_buffer()?;
        buffer
            .marked_range
            .as_ref()
            .map(|range| buffer.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(buffer) = self.active_buffer_mut() {
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
        let Some(buffer) = self.active_buffer_mut() else {
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
        let Some(buffer) = self.active_buffer_mut() else {
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
        let buffer = self.active_buffer()?;
        let range = buffer.range_from_utf16(&range_utf16);
        let layout = self.layout.as_ref()?;
        if !layout.matches_file(&self.active_file) {
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
        let buffer = self.active_buffer()?;
        let layout = self.layout.as_ref()?;
        if !layout.matches_file(&self.active_file) {
            return None;
        }
        Some(buffer.offset_to_utf16(layout.offset_for_point(point)))
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();

        let mut body = div()
            .id("editor-code-view")
            .track_focus(&self.focus_handle)
            .track_scroll(&self.scroll_handle)
            .flex_1()
            .overflow_y_scroll()
            .font_family("monospace")
            .text_size(px(13.))
            .py(px(10.));

        body = match self.active_buffer() {
            Some(_) => {
                let text_element = EditorTextElement {
                    editor_view: cx.entity(),
                    theme,
                    focus_handle: self.focus_handle.clone(),
                    scroll_handle: self.scroll_handle.clone(),
                };
                body.key_context("Editor")
                    .on_action(cx.listener(Self::backspace))
                    .on_action(cx.listener(Self::delete))
                    .on_action(cx.listener(Self::insert_newline))
                    .on_action(cx.listener(Self::move_left))
                    .on_action(cx.listener(Self::move_right))
                    .on_action(cx.listener(Self::move_up))
                    .on_action(cx.listener(Self::move_down))
                    .on_action(cx.listener(Self::select_left))
                    .on_action(cx.listener(Self::select_right))
                    .on_action(cx.listener(Self::select_up))
                    .on_action(cx.listener(Self::select_down))
                    .on_action(cx.listener(Self::select_all))
                    .on_action(cx.listener(Self::home))
                    .on_action(cx.listener(Self::end))
                    .on_action(cx.listener(Self::paste))
                    .on_action(cx.listener(Self::cut))
                    .on_action(cx.listener(Self::copy))
                    .on_action(cx.listener(Self::save_active))
                    .on_action(cx.listener(Self::undo))
                    .on_action(cx.listener(Self::redo))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(Self::on_editor_mouse_down),
                    )
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(Self::on_editor_mouse_up),
                    )
                    .on_mouse_up_out(
                        gpui::MouseButton::Left,
                        cx.listener(Self::on_editor_mouse_up),
                    )
                    .on_mouse_move(cx.listener(Self::on_editor_mouse_move))
                    .child(text_element)
            }
            None => body.child(
                div()
                    .px(px(16.))
                    .py(px(20.))
                    .text_color(theme.text_faint)
                    .child("No request open — press ⌘P to open one."),
            ),
        };

        body
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn valid_request_toml_is_written_verbatim_and_clears_dirty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        let text =
            "# a comment\n[request]\nname = \"A\"\nmethod = \"GET\"\nurl = \"https://x.test\"\n";
        std::fs::write(&path, "placeholder").unwrap();

        let file = ActiveFile::Request(path.clone());
        let mut buffers = HashMap::new();
        buffers.insert(file.clone(), EditorBuffer::new(text.to_string()));

        let saved = save_buffer_to_disk(&mut buffers, &file, None);

        assert!(saved);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert!(!buffers[&file].is_dirty());
        assert!(buffers[&file].save_error.is_none());
    }

    #[test]
    fn invalid_request_toml_is_not_written_and_sets_save_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        std::fs::write(&path, "original").unwrap();

        let file = ActiveFile::Request(path.clone());
        let mut buffer = EditorBuffer::new("name = \"A\"\n".to_string());
        buffer.replace_range(0..0, "not valid [ toml\n");
        let mut buffers = HashMap::new();
        buffers.insert(file.clone(), buffer);

        let saved = save_buffer_to_disk(&mut buffers, &file, None);

        assert!(!saved);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
        assert!(buffers[&file].is_dirty());
        assert!(buffers[&file].save_error.is_some());
    }

    #[test]
    fn valid_folder_toml_is_saved_to_folder_toml_in_the_given_dir() {
        let dir = tempdir().unwrap();
        let file = ActiveFile::Folder(dir.path().to_path_buf());
        let text = "[[headers]]\nname = \"X\"\nvalue = \"1\"\n";
        let mut buffers = HashMap::new();
        buffers.insert(file.clone(), EditorBuffer::new(text.to_string()));

        let saved = save_buffer_to_disk(&mut buffers, &file, None);

        assert!(saved);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("folder.toml")).unwrap(),
            text
        );
        assert!(!buffers[&file].is_dirty());
    }

    #[test]
    fn valid_environment_toml_is_saved_under_the_collection_environments_dir() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("environments")).unwrap();

        let file = ActiveFile::Environment("dev".to_string());
        let text = "[variables]\nbase_url = \"https://dev.test\"\n";
        let mut buffers = HashMap::new();
        buffers.insert(file.clone(), EditorBuffer::new(text.to_string()));

        let saved = save_buffer_to_disk(&mut buffers, &file, Some(dir.path()));

        assert!(saved);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("environments").join("dev.toml")).unwrap(),
            text
        );
        assert!(!buffers[&file].is_dirty());
    }

    #[test]
    fn clean_buffer_reloads_when_file_changes_on_disk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        std::fs::write(&path, "new content").unwrap();

        let file = ActiveFile::Request(path.clone());
        let mut buffers = HashMap::new();
        buffers.insert(file.clone(), EditorBuffer::new("old content".to_string()));

        let reloaded = reload_touched_buffers(&mut buffers, std::slice::from_ref(&path), None);

        assert_eq!(reloaded, vec![path]);
        assert_eq!(buffers[&file].text, "new content");
        assert!(!buffers[&file].is_dirty());
    }

    #[test]
    fn dirty_buffer_flags_external_change_without_overwriting() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        std::fs::write(&path, "disk content").unwrap();

        let file = ActiveFile::Request(path.clone());
        let mut buffer = EditorBuffer::new("original".to_string());
        buffer.replace_range(0..0, "unsaved edit ");
        let mut buffers = HashMap::new();
        buffers.insert(file.clone(), buffer);

        let reloaded = reload_touched_buffers(&mut buffers, std::slice::from_ref(&path), None);

        assert!(reloaded.is_empty());
        let buffer = &buffers[&file];
        assert!(buffer.text.starts_with("unsaved edit"));
        assert!(buffer.external_change);
    }

    #[test]
    fn matching_text_is_a_no_op_and_preserves_undo_history() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");

        let mut buffer = EditorBuffer::new("abc".to_string());
        buffer.replace_range(3..3, "d");
        buffer.mark_saved();
        std::fs::write(&path, &buffer.text).unwrap();

        let file = ActiveFile::Request(path.clone());
        let mut buffers = HashMap::new();
        buffers.insert(file.clone(), buffer);

        reload_touched_buffers(&mut buffers, std::slice::from_ref(&path), None);

        let buffer = buffers.get_mut(&file).unwrap();
        buffer.undo();
        assert_eq!(buffer.text, "abc");
    }

    #[test]
    fn untouched_path_is_ignored() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        std::fs::write(&path, "disk").unwrap();
        let other = dir.path().join("other.req.toml");

        let file = ActiveFile::Request(path);
        let mut buffers = HashMap::new();
        buffers.insert(file.clone(), EditorBuffer::new("buffer text".to_string()));

        reload_touched_buffers(&mut buffers, &[other], None);

        assert_eq!(buffers[&file].text, "buffer text");
    }
}
