use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone)]
pub struct EditorBuffer {
    pub text: String,
    original: String,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub marked_range: Option<Range<usize>>,
    pub save_error: Option<String>,
}

impl EditorBuffer {
    pub fn new(text: String) -> Self {
        Self {
            original: text.clone(),
            text,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            save_error: None,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.text != self.original
    }

    pub fn mark_saved(&mut self) {
        self.original = self.text.clone();
        self.save_error = None;
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
        self.text.replace_range(range.clone(), new_text);
        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.save_error = None;
    }

    pub fn replace_active_range(&mut self, new_text: &str) {
        let range = self
            .marked_range
            .clone()
            .unwrap_or_else(|| self.selected_range.clone());
        self.replace_range(range, new_text);
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

    pub fn line_start_offsets(&self) -> Vec<usize> {
        let mut starts = vec![0];
        for (i, byte) in self.text.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    pub fn line_col_for_offset(&self, offset: usize) -> (usize, usize) {
        let starts = self.line_start_offsets();
        let line = starts.partition_point(|&start| start <= offset).max(1) - 1;
        (line, offset - starts[line])
    }

    pub fn offset_for_line_col(&self, line: usize, col: usize) -> usize {
        let starts = self.line_start_offsets();
        let line = line.min(starts.len() - 1);
        let line_start = starts[line];
        let line_end = starts
            .get(line + 1)
            .map(|&next| next - 1)
            .unwrap_or(self.text.len());
        line_start.saturating_add(col).min(line_end)
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
        assert_eq!(
            EditorBuffer::new(String::new()).line_start_offsets(),
            vec![0]
        );
        assert_eq!(
            EditorBuffer::new("one line".to_string()).line_start_offsets(),
            vec![0]
        );
        let buffer = EditorBuffer::new("a\nbb\nccc".to_string());
        assert_eq!(buffer.line_start_offsets(), vec![0, 2, 5]);
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
    fn boundary_navigation_skips_whole_graphemes() {
        let buffer = EditorBuffer::new("abc".to_string());
        assert_eq!(buffer.next_boundary(0), 1);
        assert_eq!(buffer.next_boundary(3), 3);
        assert_eq!(buffer.previous_boundary(3), 2);
        assert_eq!(buffer.previous_boundary(0), 0);
    }
}
