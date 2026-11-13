//! The editing model behind [`super::TextField`]: one line of text, a selection and the span an
//! input method is still composing. Offsets are UTF-8 byte indices kept on grapheme boundaries;
//! the platform input handler speaks UTF-16, so conversions live here too.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Default)]
pub(crate) struct TextBuffer {
    text: String,
    selection: Range<usize>,
    /// The caret is at `selection.start` rather than `selection.end`: the selection was made
    /// leftwards.
    reversed: bool,
    /// Text an input method has inserted but not committed (pinyin before a candidate is
    /// picked). Replaced wholesale by the next composition update or the commit.
    marked: Option<Range<usize>>,
}

impl TextBuffer {
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    pub(crate) fn reversed(&self) -> bool {
        self.reversed
    }

    pub(crate) fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub(crate) fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    pub(crate) fn selected_text(&self) -> &str {
        &self.text[self.selection.clone()]
    }

    /// Replaces the whole text, caret at the end, as when a field is filled programmatically.
    pub(crate) fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.selection = self.text.len()..self.text.len();
        self.reversed = false;
        self.marked = None;
    }

    pub(crate) fn unmark(&mut self) {
        self.marked = None;
    }

    pub(crate) fn move_to(&mut self, offset: usize) {
        let offset = self.clamp(offset);
        self.selection = offset..offset;
        self.reversed = false;
    }

    /// Moves the caret end of the selection to `offset`, keeping the other end anchored.
    pub(crate) fn select_to(&mut self, offset: usize) {
        let offset = self.clamp(offset);
        if self.reversed {
            self.selection.start = offset;
        } else {
            self.selection.end = offset;
        }
        if self.selection.end < self.selection.start {
            self.reversed = !self.reversed;
            self.selection = self.selection.end..self.selection.start;
        }
    }

    pub(crate) fn select_all(&mut self) {
        self.selection = 0..self.text.len();
        self.reversed = false;
    }

    /// Selects the word under `offset`, or the run of spaces or punctuation there.
    pub(crate) fn select_word_at(&mut self, offset: usize) {
        let offset = self.clamp(offset);
        let range = self
            .text
            .split_word_bound_indices()
            .map(|(start, word)| start..start + word.len())
            .find(|range| range.contains(&offset))
            .or_else(|| {
                self.text
                    .split_word_bound_indices()
                    .next_back()
                    .map(|(start, word)| start..start + word.len())
            })
            .unwrap_or(0..0);
        self.selection = range;
        self.reversed = false;
    }

    /// Left/right without shift: collapses a selection to its edge, otherwise steps one
    /// grapheme.
    pub(crate) fn move_left(&mut self) {
        if self.selection.is_empty() {
            self.move_to(self.prev_grapheme(self.cursor()));
        } else {
            self.move_to(self.selection.start);
        }
    }

    pub(crate) fn move_right(&mut self) {
        if self.selection.is_empty() {
            self.move_to(self.next_grapheme(self.cursor()));
        } else {
            self.move_to(self.selection.end);
        }
    }

    pub(crate) fn prev_grapheme(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    pub(crate) fn next_grapheme(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.text.len())
    }

    /// Start of the word before `offset`, skipping any spaces and punctuation in between.
    /// Han characters each count as a word, per Unicode word boundaries.
    pub(crate) fn prev_word(&self, offset: usize) -> usize {
        self.text
            .split_word_bound_indices()
            .rev()
            .find(|(start, word)| *start < offset && is_word(word))
            .map_or(0, |(start, _)| start)
    }

    /// End of the word after `offset`, skipping any spaces and punctuation in between.
    pub(crate) fn next_word(&self, offset: usize) -> usize {
        self.text
            .split_word_bound_indices()
            .find(|(start, word)| start + word.len() > offset && is_word(word))
            .map_or(self.text.len(), |(start, word)| start + word.len())
    }

    /// Replaces `range`, or else the composing text, or else the selection, with `text` and
    /// leaves the caret after it. This is both typing and committing a composition.
    pub(crate) fn replace(&mut self, range: Option<Range<usize>>, text: &str) {
        let range = self.edit_range(range);
        self.text.replace_range(range.clone(), text);
        self.move_to(range.start + text.len());
        self.marked = None;
    }

    /// Like [`Self::replace`], but `text` stays marked as composing. `selected`, relative to
    /// `text`, is where the input method wants the caret inside it.
    pub(crate) fn replace_and_mark(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
    ) {
        let range = self.edit_range(range);
        self.text.replace_range(range.clone(), text);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        let selected = selected.unwrap_or(text.len()..text.len());
        self.selection =
            self.clamp(range.start + selected.start)..self.clamp(range.start + selected.end);
        self.reversed = false;
    }

    /// Deletes the selection, or else from `to` up to the caret (either direction). Returns
    /// whether anything changed.
    pub(crate) fn delete_to(&mut self, to: usize) -> bool {
        if self.selection.is_empty() {
            let cursor = self.cursor();
            let to = self.clamp(to);
            if to == cursor {
                return false;
            }
            self.selection = to.min(cursor)..to.max(cursor);
        }
        self.replace(None, "");
        true
    }

    pub(crate) fn text_to_utf16(&self, offset: usize) -> usize {
        self.text[..self.clamp(offset)].encode_utf16().count()
    }

    pub(crate) fn text_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (index, ch) in self.text.char_indices() {
            if utf16 >= offset {
                return index;
            }
            utf16 += ch.len_utf16();
        }
        self.text.len()
    }

    pub(crate) fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.text_to_utf16(range.start)..self.text_to_utf16(range.end)
    }

    pub(crate) fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.text_from_utf16(range.start)..self.text_from_utf16(range.end)
    }

    fn edit_range(&self, range: Option<Range<usize>>) -> Range<usize> {
        let range = range
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        self.clamp(range.start)..self.clamp(range.end.max(range.start))
    }

    /// Clamps to the text and backs off to a char boundary, so a stale offset can never split
    /// a code point.
    fn clamp(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }
}

fn is_word(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> TextBuffer {
        let mut buffer = TextBuffer::default();
        buffer.set_text(text);
        buffer
    }

    #[test]
    fn typing_inserts_at_the_caret_and_replaces_a_selection() {
        let mut b = buffer("hello");
        b.move_to(2);
        b.replace(None, "XY");
        assert_eq!(b.text(), "heXYllo");
        assert_eq!(b.cursor(), 4);

        b.select_all();
        b.replace(None, "周杰伦");
        assert_eq!(b.text(), "周杰伦");
        assert_eq!(b.cursor(), "周杰伦".len());
    }

    #[test]
    fn backspace_and_delete_step_whole_graphemes() {
        let mut b = buffer("a👩‍👩‍👧b");
        b.move_to("a👩‍👩‍👧".len());
        assert!(b.delete_to(b.prev_grapheme(b.cursor())));
        assert_eq!(b.text(), "ab");

        let mut b = buffer("é是");
        b.move_to(0);
        assert!(b.delete_to(b.next_grapheme(b.cursor())));
        assert_eq!(b.text(), "是");
    }

    #[test]
    fn deleting_at_an_edge_changes_nothing() {
        let mut b = buffer("ab");
        b.move_to(0);
        assert!(!b.delete_to(b.prev_grapheme(0)));
        b.move_to(2);
        assert!(!b.delete_to(b.next_grapheme(2)));
        assert_eq!(b.text(), "ab");
    }

    #[test]
    fn word_delete_takes_the_previous_word_and_the_space_after_it() {
        let mut b = buffer("taylor swift  ");
        assert!(b.delete_to(b.prev_word(b.cursor())));
        assert_eq!(b.text(), "taylor ");
        assert!(b.delete_to(b.prev_word(b.cursor())));
        assert_eq!(b.text(), "");
    }

    #[test]
    fn word_delete_skips_punctuation_and_treats_han_characters_as_words() {
        let mut b = buffer("rock, roll!");
        assert!(b.delete_to(b.prev_word(b.cursor())));
        assert_eq!(b.text(), "rock, ");

        let mut b = buffer("周杰伦 jay");
        b.move_to("周杰伦".len());
        assert!(b.delete_to(b.prev_word(b.cursor())));
        assert_eq!(b.text(), "周杰 jay");
    }

    #[test]
    fn word_delete_forward_stops_at_the_end_of_the_next_word() {
        let mut b = buffer("one  two three");
        b.move_to(3);
        assert!(b.delete_to(b.next_word(b.cursor())));
        assert_eq!(b.text(), "one three");
    }

    #[test]
    fn deleting_with_a_selection_removes_only_the_selection() {
        let mut b = buffer("one two three");
        b.move_to(4);
        b.select_to(7);
        assert!(b.delete_to(0));
        assert_eq!(b.text(), "one  three");
        assert_eq!(b.cursor(), 4);
    }

    #[test]
    fn delete_to_line_start_clears_everything_before_the_caret() {
        let mut b = buffer("hello world");
        b.move_to(6);
        assert!(b.delete_to(0));
        assert_eq!(b.text(), "world");
        assert_eq!(b.cursor(), 0);
    }

    #[test]
    fn selecting_leftwards_past_the_anchor_flips_the_selection() {
        let mut b = buffer("abcdef");
        b.move_to(3);
        b.select_to(5);
        assert_eq!((b.selection(), b.reversed()), (3..5, false));
        b.select_to(1);
        assert_eq!((b.selection(), b.reversed()), (1..3, true));
        assert_eq!(b.cursor(), 1);
        assert_eq!(b.selected_text(), "bc");
    }

    #[test]
    fn arrows_collapse_a_selection_to_its_edge() {
        let mut b = buffer("abcdef");
        b.move_to(1);
        b.select_to(4);
        b.move_left();
        assert_eq!(b.selection(), 1..1);
        b.select_to(4);
        b.move_right();
        assert_eq!(b.selection(), 4..4);
    }

    #[test]
    fn double_click_selects_the_word_under_the_pointer() {
        let mut b = buffer("hello brave world");
        b.select_word_at(8);
        assert_eq!(b.selected_text(), "brave");
        b.select_word_at(100);
        assert_eq!(b.selected_text(), "world");
    }

    #[test]
    fn pinyin_composition_is_marked_then_replaced_by_the_chosen_characters() {
        let mut b = buffer("听 ");
        b.replace_and_mark(None, "zhou", None);
        assert_eq!(b.text(), "听 zhou");
        assert_eq!(b.marked(), Some("听 ".len().."听 zhou".len()));

        b.replace_and_mark(None, "zhou jie", Some(8..8));
        assert_eq!(b.text(), "听 zhou jie");
        assert_eq!(b.cursor(), "听 zhou jie".len());

        b.replace(None, "周杰");
        assert_eq!(b.text(), "听 周杰");
        assert_eq!(b.marked(), None);
        assert_eq!(b.cursor(), b.text().len());
    }

    #[test]
    fn an_empty_composition_update_clears_the_mark() {
        let mut b = buffer("");
        b.replace_and_mark(None, "n", None);
        b.replace_and_mark(None, "", None);
        assert_eq!(b.text(), "");
        assert_eq!(b.marked(), None);
    }

    #[test]
    fn utf16_offsets_round_trip_through_astral_characters() {
        let b = buffer("a😀周");
        assert_eq!(b.text_to_utf16("a😀".len()), 3);
        assert_eq!(b.text_from_utf16(3), "a😀".len());
        assert_eq!(b.range_from_utf16(&(0..4)), 0..b.text().len());
        assert_eq!(b.text_from_utf16(99), b.text().len());
    }

    #[test]
    fn out_of_range_offsets_are_clamped_to_a_char_boundary() {
        let mut b = buffer("周");
        b.move_to(1);
        assert_eq!(b.cursor(), 0);
        b.replace(Some(1..99), "x");
        assert_eq!(b.text(), "x");
    }
}
