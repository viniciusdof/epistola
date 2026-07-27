use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Toml,
    Json,
    Xml,
    PlainText,
}

#[derive(Debug, Clone)]
struct EditRecord {
    start: usize,
    removed: String,
    inserted: String,
    selection_before: Range<usize>,
    selection_reversed_before: bool,
}

#[derive(Debug, Clone)]
struct ComposingOrigin {
    start: usize,
    removed: String,
    selection_before: Range<usize>,
    selection_reversed_before: bool,
}

#[derive(Debug, Clone)]
pub struct EditorBuffer {
    pub text: String,
    original: String,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub marked_range: Option<Range<usize>>,
    pub save_error: Option<String>,
    pub external_change: bool,
    pub read_only: bool,
    pub content_kind: ContentKind,
    line_starts: Vec<usize>,
    pub desired_col: Option<usize>,
    undo_stack: Vec<EditRecord>,
    redo_stack: Vec<EditRecord>,
    composing_origin: Option<ComposingOrigin>,
}

impl EditorBuffer {
    pub fn new(text: String) -> Self {
        let mut buffer = Self {
            original: text.clone(),
            text,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            save_error: None,
            external_change: false,
            read_only: false,
            content_kind: ContentKind::Toml,
            line_starts: Vec::new(),
            desired_col: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            composing_origin: None,
        };
        buffer.recompute_line_starts();
        buffer
    }

    pub fn read_only(text: String) -> Self {
        Self {
            read_only: true,
            ..Self::new(text)
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.text != self.original
    }

    pub fn mark_saved(&mut self) {
        self.original = self.text.clone();
        self.save_error = None;
        self.external_change = false;
    }

    pub fn set_text(&mut self, text: String) {
        self.text = text;
        self.recompute_line_starts();
        let end = self.text.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        self.save_error = None;
        self.external_change = false;
        self.desired_col = None;
        self.composing_origin = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    pub fn move_to(&mut self, offset: usize) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
    }

    pub fn select_to(&mut self, offset: usize) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
    }

    pub fn replace_range(&mut self, range: Range<usize>, new_text: &str) {
        let (record_start, removed, selection_before, selection_reversed_before) =
            match self.composing_origin.take() {
                Some(origin) => (
                    origin.start,
                    origin.removed,
                    origin.selection_before,
                    origin.selection_reversed_before,
                ),
                None => (
                    range.start,
                    self.text[range.clone()].to_string(),
                    self.selected_range.clone(),
                    self.selection_reversed,
                ),
            };
        self.text.replace_range(range.clone(), new_text);
        self.recompute_line_starts();
        let inserted_end = range.start + new_text.len();
        let inserted = self.text[record_start..inserted_end].to_string();
        self.push_undo_record(EditRecord {
            start: record_start,
            removed,
            inserted,
            selection_before,
            selection_reversed_before,
        });
        let cursor = inserted_end;
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.save_error = None;
        self.desired_col = None;
    }

    pub fn replace_active_range(&mut self, new_text: &str) {
        let range = self
            .marked_range
            .clone()
            .unwrap_or_else(|| self.selected_range.clone());
        self.replace_range(range, new_text);
    }

    pub fn set_marked_text(&mut self, range: Range<usize>, new_text: &str) {
        if self.composing_origin.is_none() {
            self.composing_origin = Some(ComposingOrigin {
                start: range.start,
                removed: self.text[range.clone()].to_string(),
                selection_before: self.selected_range.clone(),
                selection_reversed_before: self.selection_reversed,
            });
        }
        self.text.replace_range(range, new_text);
        self.recompute_line_starts();
        self.save_error = None;
    }

    pub fn cancel_composition(&mut self) {
        self.composing_origin = None;
    }

    fn push_undo_record(&mut self, record: EditRecord) {
        self.redo_stack.clear();
        let is_simple_insert = record.removed.is_empty()
            && record.inserted.graphemes(true).count() == 1
            && !record
                .inserted
                .chars()
                .next()
                .is_some_and(char::is_whitespace);
        if is_simple_insert {
            if let Some(last) = self.undo_stack.last_mut() {
                let mergeable = last.removed.is_empty()
                    && last.start + last.inserted.len() == record.start
                    && last
                        .inserted
                        .chars()
                        .next_back()
                        .is_some_and(|c| !c.is_whitespace());
                if mergeable {
                    last.inserted.push_str(&record.inserted);
                    return;
                }
            }
        }
        self.undo_stack.push(record);
    }

    pub fn undo(&mut self) {
        let Some(record) = self.undo_stack.pop() else {
            return;
        };
        let edited_range = record.start..record.start + record.inserted.len();
        self.text.replace_range(edited_range, &record.removed);
        self.recompute_line_starts();
        self.selected_range = record.selection_before.clone();
        self.selection_reversed = record.selection_reversed_before;
        self.marked_range = None;
        self.save_error = None;
        self.desired_col = None;
        self.redo_stack.push(record);
    }

    pub fn redo(&mut self) {
        let Some(record) = self.redo_stack.pop() else {
            return;
        };
        let edited_range = record.start..record.start + record.removed.len();
        self.text.replace_range(edited_range, &record.inserted);
        self.recompute_line_starts();
        let cursor = record.start + record.inserted.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.save_error = None;
        self.desired_col = None;
        self.undo_stack.push(record);
    }

    pub fn previous_boundary(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    pub fn next_boundary(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.text.len())
    }

    fn recompute_line_starts(&mut self) {
        self.line_starts.clear();
        self.line_starts.push(0);
        for (i, byte) in self.text.bytes().enumerate() {
            if byte == b'\n' {
                self.line_starts.push(i + 1);
            }
        }
    }

    pub fn line_start_offsets(&self) -> &[usize] {
        &self.line_starts
    }

    pub fn line_col_for_offset(&self, offset: usize) -> (usize, usize) {
        let starts = &self.line_starts;
        let line = starts.partition_point(|&start| start <= offset).max(1) - 1;
        (line, offset - starts[line])
    }

    pub fn offset_for_line_col(&self, line: usize, col: usize) -> usize {
        let starts = &self.line_starts;
        let line = line.min(starts.len() - 1);
        let line_start = starts[line];
        let line_end = starts
            .get(line + 1)
            .map(|&next| next - 1)
            .unwrap_or(self.text.len());
        line_start.saturating_add(col).min(line_end)
    }

    pub fn offset_vertical(&mut self, delta: isize) -> usize {
        let (line, col) = self.line_col_for_offset(self.cursor_offset());
        let goal = *self.desired_col.get_or_insert(col);
        match line.checked_add_signed(delta) {
            Some(target_line) => self.offset_for_line_col(target_line, goal),
            None => 0,
        }
    }

    pub fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..offset].chars().map(char::len_utf16).sum()
    }

    pub fn offset_from_utf16(&self, utf16_offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.text.chars() {
            if utf16_count >= utf16_offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    pub fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    pub fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn new_buffer_is_not_dirty() {
        let buffer = EditorBuffer::new("hello".to_string());
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn replace_range_marks_dirty_and_moves_cursor() {
        let mut buffer = EditorBuffer::new("hello".to_string());
        buffer.replace_range(1..1, "X");
        assert_eq!(buffer.text, "hXello");
        assert_eq!(buffer.selected_range, 2..2);
        assert!(buffer.is_dirty());
    }

    #[test]
    fn mark_saved_clears_dirty_and_error() {
        let mut buffer = EditorBuffer::new("hello".to_string());
        buffer.replace_range(0..0, "X");
        buffer.save_error = Some("boom".to_string());
        buffer.mark_saved();
        assert!(!buffer.is_dirty());
        assert!(buffer.save_error.is_none());
    }

    #[test]
    fn mark_saved_clears_external_change() {
        let mut buffer = EditorBuffer::new("hello".to_string());
        buffer.external_change = true;
        buffer.mark_saved();
        assert!(!buffer.external_change);
    }

    #[test]
    fn replace_range_across_multibyte_boundary() {
        let mut buffer = EditorBuffer::new("héllo".to_string());
        let e_start = "h".len();
        let e_end = e_start + "é".len();
        buffer.replace_range(e_start..e_end, "e");
        assert_eq!(buffer.text, "hello");
    }

    #[test]
    fn utf16_offset_translation_roundtrips_through_emoji() {
        let buffer = EditorBuffer::new("a😀b".to_string());
        let emoji_start_utf8 = "a".len();
        let emoji_end_utf8 = emoji_start_utf8 + "😀".len();
        // '😀' is a surrogate pair in UTF-16: 2 units.
        assert_eq!(buffer.offset_to_utf16(emoji_start_utf8), 1);
        assert_eq!(buffer.offset_to_utf16(emoji_end_utf8), 3);
        assert_eq!(buffer.offset_from_utf16(1), emoji_start_utf8);
        assert_eq!(buffer.offset_from_utf16(3), emoji_end_utf8);
    }

    #[test]
    fn line_start_offsets_on_empty_single_and_multi_line_text() {
        assert_eq!(EditorBuffer::new(String::new()).line_start_offsets(), [0]);
        assert_eq!(
            EditorBuffer::new("one line".to_string()).line_start_offsets(),
            [0]
        );
        let buffer = EditorBuffer::new("a\nbb\nccc".to_string());
        assert_eq!(buffer.line_start_offsets(), [0, 2, 5]);
    }

    #[test]
    fn line_starts_cache_invalidated_on_edit() {
        let mut buffer = EditorBuffer::new("abc".to_string());
        assert_eq!(buffer.line_start_offsets(), [0]);
        buffer.replace_range(1..1, "\n");
        assert_eq!(buffer.line_start_offsets(), [0, 2]);
        buffer.undo();
        assert_eq!(buffer.line_start_offsets(), [0]);
    }

    #[test]
    fn line_col_for_offset_and_back() {
        let buffer = EditorBuffer::new("a\nbb\nccc".to_string());
        assert_eq!(buffer.line_col_for_offset(0), (0, 0));
        assert_eq!(buffer.line_col_for_offset(2), (1, 0));
        assert_eq!(buffer.line_col_for_offset(4), (1, 2));
        assert_eq!(buffer.line_col_for_offset(7), (2, 2));

        assert_eq!(buffer.offset_for_line_col(1, 0), 2);
        assert_eq!(buffer.offset_for_line_col(1, 5), 4); // clamps to line end
        assert_eq!(buffer.offset_for_line_col(10, 0), 5); // clamps to last line
    }

    #[test]
    fn offset_vertical_preserves_desired_column_through_a_short_line() {
        let mut buffer = EditorBuffer::new("abcdef\nxy\nuvwxyz".to_string());
        buffer.move_to(5);
        let down1 = buffer.offset_vertical(1);
        buffer.move_to(down1);
        assert_eq!(buffer.line_col_for_offset(down1), (1, 2)); // clamped to short line's end
        let down2 = buffer.offset_vertical(1);
        buffer.move_to(down2);
        assert_eq!(buffer.line_col_for_offset(down2), (2, 5)); // restored once room exists
    }

    #[test]
    fn offset_vertical_resets_when_desired_col_cleared() {
        let mut buffer = EditorBuffer::new("abcdef\nxy".to_string());
        buffer.move_to(5);
        let _ = buffer.offset_vertical(1);
        buffer.desired_col = None; // simulates a horizontal movement resetting it
        buffer.move_to(1);
        assert_eq!(buffer.offset_vertical(-1), 0); // recomputed from the new column (1), not the old (5)
    }

    #[test]
    fn boundary_navigation_skips_whole_graphemes() {
        let buffer = EditorBuffer::new("abc".to_string());
        assert_eq!(buffer.next_boundary(0), 1);
        assert_eq!(buffer.next_boundary(3), 3);
        assert_eq!(buffer.previous_boundary(3), 2);
        assert_eq!(buffer.previous_boundary(0), 0);
    }

    #[test]
    fn typing_a_word_coalesces_into_one_undo_step() {
        let mut buffer = EditorBuffer::new(String::new());
        for ch in ['h', 'i'] {
            let cursor = buffer.cursor_offset();
            buffer.replace_range(cursor..cursor, &ch.to_string());
        }
        assert_eq!(buffer.text, "hi");
        buffer.undo();
        assert_eq!(buffer.text, "");
        assert_eq!(buffer.selected_range, 0..0);
    }

    #[test]
    fn typing_breaks_coalescing_on_whitespace() {
        let mut buffer = EditorBuffer::new(String::new());
        for ch in ['h', 'i', ' ', 'a'] {
            let cursor = buffer.cursor_offset();
            buffer.replace_range(cursor..cursor, &ch.to_string());
        }
        assert_eq!(buffer.text, "hi a");
        buffer.undo();
        assert_eq!(buffer.text, "hi ");
        buffer.undo();
        assert_eq!(buffer.text, "hi");
        buffer.undo();
        assert_eq!(buffer.text, "");
    }

    #[test]
    fn undo_restores_selection_and_redo_reapplies_edit() {
        let mut buffer = EditorBuffer::new("hello world".to_string());
        buffer.selected_range = 0..5;
        buffer.replace_range(0..5, "bye");
        assert_eq!(buffer.text, "bye world");
        buffer.undo();
        assert_eq!(buffer.text, "hello world");
        assert_eq!(buffer.selected_range, 0..5);
        buffer.redo();
        assert_eq!(buffer.text, "bye world");
        assert_eq!(buffer.selected_range, 3..3);
    }

    #[test]
    fn redo_stack_clears_on_new_edit() {
        let mut buffer = EditorBuffer::new("abc".to_string());
        buffer.replace_range(3..3, "d");
        buffer.undo();
        buffer.replace_range(0..0, "x");
        buffer.redo(); // no-op: redo history was discarded by the edit above
        assert_eq!(buffer.text, "xabc");
    }

    #[test]
    fn undo_survives_save() {
        let mut buffer = EditorBuffer::new("abc".to_string());
        buffer.replace_range(3..3, "d");
        buffer.mark_saved();
        assert!(!buffer.is_dirty());
        buffer.undo();
        assert_eq!(buffer.text, "abc");
    }

    #[test]
    fn ime_composition_collapses_into_a_single_undo_record() {
        let mut buffer = EditorBuffer::new("x".to_string());
        buffer.move_to(1);
        // Simulates typing romaji "on" then converting: two marked-text updates, then commit.
        buffer.set_marked_text(1..1, "o");
        buffer.marked_range = Some(1..2);
        buffer.selected_range = 2..2;
        buffer.set_marked_text(1..2, "on");
        buffer.marked_range = Some(1..3);
        buffer.selected_range = 3..3;
        buffer.replace_range(1..3, "\u{6771}"); // commits the composed kanji
        assert_eq!(buffer.text, "x\u{6771}");
        assert_eq!(buffer.undo_stack.len(), 1);
        buffer.undo();
        assert_eq!(buffer.text, "x");
        assert_eq!(buffer.selected_range, 1..1);
    }
}
