//! The multi-line input buffer holding committed text.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const TAB_STOP: usize = 8;
const UNDO_LIMIT: usize = 1000;

/// How grapheme `g` is drawn at display column `col`, with its width: a tab
/// extends to the next tab stop, other control characters use caret notation
/// (`^M`) so no raw control byte reaches the terminal, and nothing is
/// narrower than one column. Not for `\n`, which ends the line.
pub fn cell(g: &str, col: usize) -> (Cow<'_, str>, usize) {
    if g == "\t" {
        let w = TAB_STOP - col % TAB_STOP;
        return (" ".repeat(w).into(), w);
    }
    let mut chars = g.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_control()
    {
        let s = match c {
            '\0'..='\x1f' => format!("^{}", (c as u8 ^ 0x40) as char),
            '\x7f' => "^?".into(),
            _ => "\u{fffd}".into(),
        };
        let w = s.width();
        return (s.into(), w);
    }
    (g.into(), g.width().max(1))
}

/// Horizontal whitespace; a line break is never blank.
fn is_blank(s: &str) -> bool {
    s.chars().all(|c| c.is_whitespace() && c != '\n')
}

/// Which consecutive edits merge into one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Other,
    Typing,
    Backspace,
    Delete,
}

/// `removed` at byte offset `at` was replaced by `inserted`.
#[derive(Debug, Clone)]
struct Change {
    at: usize,
    removed: String,
    inserted: String,
    cursor_before: usize,
    cursor_after: usize,
}

#[derive(Debug, Default, Clone)]
struct UndoLog {
    undo: VecDeque<Change>,
    redo: Vec<Change>,
    /// The kind of the last step while edits of that kind may extend it.
    open: Option<Kind>,
}

impl UndoLog {
    fn record(&mut self, c: Change, kind: Kind) {
        self.redo.clear();
        let open = self.open.filter(|k| *k == kind);
        match (open, self.undo.back_mut()) {
            (Some(Kind::Typing), Some(top)) if c.at == top.at + top.inserted.len() => {
                top.inserted.push_str(&c.inserted);
                top.cursor_after = c.cursor_after;
            }
            (Some(Kind::Backspace), Some(top)) if c.at + c.removed.len() == top.at => {
                top.removed.insert_str(0, &c.removed);
                top.at = c.at;
                top.cursor_after = c.cursor_after;
            }
            (Some(Kind::Delete), Some(top)) if c.at == top.at => {
                top.removed.push_str(&c.removed);
                top.cursor_after = c.cursor_after;
            }
            _ => {
                if self.undo.len() == UNDO_LIMIT {
                    self.undo.pop_front();
                }
                self.undo.push_back(c);
            }
        }
        self.open = (kind != Kind::Other).then_some(kind);
    }

    /// Ends the last step: the next edit starts a new one.
    fn seal(&mut self) {
        self.open = None;
    }
}

#[derive(Debug, Default, Clone)]
pub struct Buffer {
    text: String,
    /// Byte offset, always on a grapheme boundary.
    cursor: usize,
    /// Display column kept across vertical moves.
    goal_col: Option<usize>,
    undo_log: UndoLog,
}

impl Buffer {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Deletes all text; undoable.
    pub fn clear(&mut self) {
        self.remove(0..self.text.len(), Kind::Other);
    }

    /// Replaces the text and starts a new undo history; the cursor goes to
    /// the start.
    pub fn set_text(&mut self, text: &str) {
        *self = Self {
            text: text.to_owned(),
            ..Self::default()
        };
    }

    /// Takes the text and resets the buffer, undo history included.
    pub fn take(&mut self) -> String {
        std::mem::take(self).text
    }

    /// Replaces `range` with `text`, puts the cursor at `cursor` and records
    /// the change for undo.
    fn edit(&mut self, range: Range<usize>, text: &str, cursor: usize, kind: Kind) {
        self.goal_col = None;
        if range.is_empty() && text.is_empty() {
            return;
        }
        let change = Change {
            at: range.start,
            removed: self.text[range.clone()].to_owned(),
            inserted: text.to_owned(),
            cursor_before: self.cursor,
            cursor_after: cursor,
        };
        self.text.replace_range(range, text);
        self.cursor = cursor;
        self.undo_log.record(change, kind);
    }

    /// Deletes `range`; the cursor goes to its start.
    fn remove(&mut self, range: Range<usize>, kind: Kind) {
        let at = range.start;
        self.edit(range, "", at, kind);
    }

    fn insert(&mut self, s: &str, kind: Kind) {
        self.edit(self.cursor..self.cursor, s, self.cursor + s.len(), kind);
    }

    /// Inserts `s` as its own undo step.
    pub fn insert_str(&mut self, s: &str) {
        self.insert(s, Kind::Other);
    }

    /// Inserts a typed character. Consecutive ones form one undo step, which
    /// ASCII punctuation ends.
    pub fn type_char(&mut self, c: char) {
        self.insert(c.encode_utf8(&mut [0; 4]), Kind::Typing);
        if c.is_ascii_punctuation() {
            self.undo_log.seal();
        }
    }

    pub fn newline(&mut self) {
        self.insert_str("\n");
    }

    /// Reverts the last undo step; the cursor returns to where it was
    /// before that step.
    pub fn undo(&mut self) {
        self.undo_log.seal();
        let Some(c) = self.undo_log.undo.pop_back() else {
            return;
        };
        self.text
            .replace_range(c.at..c.at + c.inserted.len(), &c.removed);
        self.cursor = c.cursor_before;
        self.goal_col = None;
        self.undo_log.redo.push(c);
    }

    /// Reapplies the last undone step.
    pub fn redo(&mut self) {
        self.undo_log.seal();
        let Some(c) = self.undo_log.redo.pop() else {
            return;
        };
        self.text
            .replace_range(c.at..c.at + c.removed.len(), &c.inserted);
        self.cursor = c.cursor_after;
        self.goal_col = None;
        self.undo_log.undo.push_back(c);
    }

    /// Moves the cursor to `at`, ending the current undo step.
    fn move_to(&mut self, at: usize) {
        self.cursor = at;
        self.goal_col = None;
        self.undo_log.seal();
    }

    fn line_start(&self) -> usize {
        self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |i| self.cursor + i)
    }

    fn prev_boundary(&self) -> Option<usize> {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
    }

    fn next_boundary(&self) -> Option<usize> {
        self.text[self.cursor..]
            .graphemes(true)
            .next()
            .map(|g| self.cursor + g.len())
    }

    pub fn left(&mut self) {
        self.move_to(self.prev_boundary().unwrap_or(self.cursor));
    }

    pub fn right(&mut self) {
        self.move_to(self.next_boundary().unwrap_or(self.cursor));
    }

    pub fn home(&mut self) {
        self.move_to(self.line_start());
    }

    pub fn end(&mut self) {
        self.move_to(self.line_end());
    }

    fn col(&self) -> usize {
        self.text[self.line_start()..self.cursor]
            .graphemes(true)
            .fold(0, |w, g| w + cell(g, w).1)
    }

    /// Byte offset in the line starting at `start` closest to display column
    /// `col` without passing it.
    fn offset_at_col(&self, start: usize, col: usize) -> usize {
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |i| start + i);
        let mut w = 0;
        for (i, g) in self.text[start..end].grapheme_indices(true) {
            let gw = cell(g, w).1;
            if w + gw > col {
                return start + i;
            }
            w += gw;
        }
        end
    }

    pub fn prev_line(&mut self) {
        let start = self.line_start();
        if start == 0 {
            return;
        }
        let goal = *self.goal_col.get_or_insert(self.col());
        let prev_start = self.text[..start - 1].rfind('\n').map_or(0, |i| i + 1);
        self.cursor = self.offset_at_col(prev_start, goal);
        self.undo_log.seal();
    }

    pub fn next_line(&mut self) {
        let end = self.line_end();
        if end == self.text.len() {
            return;
        }
        let goal = *self.goal_col.get_or_insert(self.col());
        self.cursor = self.offset_at_col(end + 1, goal);
        self.undo_log.seal();
    }

    pub fn backspace(&mut self) {
        let start = self.prev_boundary().unwrap_or(self.cursor);
        self.remove(start..self.cursor, Kind::Backspace);
    }

    pub fn delete(&mut self) {
        let end = self.next_boundary().unwrap_or(self.cursor);
        self.remove(self.cursor..end, Kind::Delete);
    }

    /// Start of the word before the cursor: trailing blanks plus one word
    /// segment (UAX #29, so each CJK ideograph is its own word). A line break
    /// directly before the cursor is a word on its own.
    fn word_start_before(&self) -> usize {
        let before = &self.text[..self.cursor];
        if let Some(rest) = before.strip_suffix('\n') {
            return rest.strip_suffix('\r').unwrap_or(rest).len();
        }
        let segs: Vec<(usize, &str)> = before.split_word_bound_indices().collect();
        let mut i = segs.len();
        while i > 0 && is_blank(segs[i - 1].1) {
            i -= 1;
        }
        if i > 0 && !segs[i - 1].1.contains('\n') {
            i -= 1;
        }
        segs.get(i).map_or(self.cursor, |(off, _)| *off)
    }

    /// End of the word after the cursor, mirroring [`Self::word_start_before`].
    fn word_end_after(&self) -> usize {
        let after = &self.text[self.cursor..];
        if let Some(n) = ["\n", "\r\n"].iter().find(|n| after.starts_with(**n)) {
            return self.cursor + n.len();
        }
        let mut segs = after.split_word_bounds().peekable();
        let mut end = self.cursor;
        while let Some(seg) = segs.next_if(|s| is_blank(s)) {
            end += seg.len();
        }
        if let Some(seg) = segs.next_if(|s| !s.contains('\n')) {
            end += seg.len();
        }
        end
    }

    pub fn word_left(&mut self) {
        self.move_to(self.word_start_before());
    }

    pub fn word_right(&mut self) {
        self.move_to(self.word_end_after());
    }

    pub fn buffer_start(&mut self) {
        self.move_to(0);
    }

    pub fn buffer_end(&mut self) {
        self.move_to(self.text.len());
    }

    /// Moves the cursor to byte offset `at`, which must be a grapheme
    /// boundary.
    pub fn set_cursor(&mut self, at: usize) {
        debug_assert!(self.text.is_char_boundary(at));
        self.move_to(at.min(self.text.len()));
    }

    /// Deletes `range`, which must lie on grapheme boundaries; the cursor
    /// goes to its start.
    pub fn delete_range(&mut self, range: Range<usize>) {
        self.remove(range, Kind::Other);
    }

    /// Deletes the word before the cursor (see [`Self::word_start_before`]).
    pub fn kill_word(&mut self) {
        self.remove(self.word_start_before()..self.cursor, Kind::Other);
    }

    /// Deletes to the start of the line; at the start, joins with the
    /// previous line.
    pub fn kill_to_start(&mut self) {
        let mut start = self.line_start();
        if start == self.cursor {
            start = self.prev_boundary().unwrap_or(start);
        }
        self.remove(start..self.cursor, Kind::Other);
    }

    /// Deletes to the end of the line; at the end, deletes the line break.
    pub fn kill_to_end(&mut self) {
        let mut end = self.line_end();
        if end == self.cursor {
            end = self.next_boundary().unwrap_or(end);
        }
        self.remove(self.cursor..end, Kind::Other);
    }

    /// (line index, byte offset within that line) of the cursor.
    #[cfg(test)]
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let line = self.text[..self.cursor].matches('\n').count();
        (line, self.cursor - self.line_start())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str) -> Buffer {
        let mut b = Buffer::default();
        b.insert_str(s);
        b
    }

    #[test]
    fn insert_and_move() {
        let mut b = buf("我们明天");
        b.left();
        b.left();
        b.insert_str("x");
        assert_eq!(b.text(), "我们x明天");
        b.home();
        assert_eq!(b.cursor(), 0);
        b.end();
        assert_eq!(b.cursor(), b.text().len());
    }

    #[test]
    fn lines_keep_display_column() {
        // Wide chars count two columns.
        let mut b = buf("你好世界\nab\n中文字");
        b.prev_line(); // from col 6 of line 2 to line 1 ("ab" has width 2)
        assert_eq!(b.cursor_line_col(), (1, 2));
        b.prev_line(); // goal col 6 is kept
        assert_eq!(b.cursor_line_col(), (0, "你好世".len()));
        b.next_line();
        b.next_line();
        assert_eq!(b.cursor_line_col(), (2, "中文字".len()));
        b.next_line(); // last line: no-op
        assert_eq!(b.cursor_line_col(), (2, "中文字".len()));
    }

    #[test]
    fn half_width_goal_does_not_split_wide_char() {
        let mut b = buf("你好\na");
        b.prev_line(); // goal col 1 falls inside 你 -> stays before it
        assert_eq!(b.cursor_line_col(), (0, 0));
    }

    #[test]
    fn backspace_joins_lines() {
        let mut b = buf("ab\ncd");
        b.home();
        b.backspace();
        assert_eq!(b.text(), "abcd");
        assert_eq!(b.cursor(), 2);
    }

    #[test]
    fn kill_to_end_and_start() {
        let mut b = buf("abc\ndef");
        b.prev_line();
        b.home();
        b.right();
        b.kill_to_end();
        assert_eq!(b.text(), "a\ndef");
        b.kill_to_end(); // at line end: removes the newline
        assert_eq!(b.text(), "adef");
        b.end();
        b.kill_to_start();
        assert_eq!(b.text(), "");
        let mut b = buf("ab\ncd");
        b.home();
        b.kill_to_start(); // at line start: joins
        assert_eq!(b.text(), "abcd");
    }

    #[test]
    fn kill_word_variants() {
        let mut b = buf("hello world  ");
        b.kill_word();
        assert_eq!(b.text(), "hello ");
        b.kill_word();
        assert_eq!(b.text(), "");
        let mut b = buf("我们明天");
        b.kill_word();
        assert_eq!(b.text(), "我们明");
        let mut b = buf("ab\ncd");
        b.kill_word();
        assert_eq!(b.text(), "ab\n");
        b.kill_word();
        assert_eq!(b.text(), "ab");
    }

    #[test]
    fn word_motion() {
        let mut b = buf("hello  world\n我们 ab");
        b.word_left();
        assert_eq!(b.cursor(), "hello  world\n我们 ".len());
        b.word_left(); // skips the blank, then one ideograph
        assert_eq!(b.cursor(), "hello  world\n我".len());
        b.word_left();
        b.word_left(); // the line break is a word on its own
        assert_eq!(b.cursor(), "hello  world".len());
        b.word_left();
        assert_eq!(b.cursor(), "hello  ".len());
        b.word_left();
        b.word_left(); // at the start: no-op
        assert_eq!(b.cursor(), 0);

        b.word_right();
        assert_eq!(b.cursor(), "hello".len());
        b.word_right(); // skips the blanks, then the word
        assert_eq!(b.cursor(), "hello  world".len());
        b.word_right();
        assert_eq!(b.cursor(), "hello  world\n".len());
        b.word_right();
        assert_eq!(b.cursor(), "hello  world\n我".len());
        b.word_right();
        b.word_right();
        b.word_right(); // at the end: no-op
        assert_eq!(b.cursor(), b.text().len());
    }

    #[test]
    fn blanks_before_line_break_stop_there() {
        let mut b = buf("ab  \ncd");
        b.buffer_start();
        b.word_right();
        b.word_right();
        assert_eq!(b.cursor(), "ab  ".len());
    }

    #[test]
    fn buffer_start_and_end() {
        let mut b = buf("ab\ncd");
        b.buffer_start();
        assert_eq!(b.cursor(), 0);
        b.buffer_end();
        assert_eq!(b.cursor(), b.text().len());
    }

    #[test]
    fn cells() {
        assert_eq!(cell("\t", 0), (Cow::from("        "), 8));
        assert_eq!(cell("\t", 3).1, 5);
        assert_eq!(cell("\t", 8).1, 8);
        assert_eq!(cell("\r", 0), (Cow::from("^M"), 2));
        assert_eq!(cell("\x1b", 0), (Cow::from("^["), 2));
        assert_eq!(cell("\x7f", 0), (Cow::from("^?"), 2));
        assert_eq!(cell("中", 5), (Cow::from("中"), 2));
    }

    #[test]
    fn tabs_keep_display_column() {
        // "#\tab" puts "a" at column 8.
        let mut b = buf("#\tab\n0123456789");
        b.prev_line();
        b.home();
        b.right();
        b.right(); // after the tab, column 8
        b.next_line();
        assert_eq!(b.cursor_line_col(), (1, 8));
        b.right(); // column 9
        b.prev_line(); // column 9 on the first line is after "a"
        assert_eq!(b.cursor_line_col(), (0, 3));
    }

    #[test]
    fn set_text_puts_cursor_at_start() {
        let mut b = buf("old");
        b.set_text("a\nb");
        assert_eq!(b.text(), "a\nb");
        assert_eq!(b.cursor(), 0);
    }

    #[test]
    fn take_resets() {
        let mut b = buf("x\ny");
        assert_eq!(b.take(), "x\ny");
        assert!(b.is_empty());
        assert_eq!(b.cursor(), 0);
        b.undo();
        assert!(b.is_empty(), "the undo history goes with the text");
    }

    fn typ(b: &mut Buffer, s: &str) {
        s.chars().for_each(|c| b.type_char(c));
    }

    #[test]
    fn typing_merges_until_punctuation() {
        let mut b = Buffer::default();
        typ(&mut b, "hello world, foo bar");
        b.undo();
        assert_eq!(b.text(), "hello world,");
        assert_eq!(b.cursor(), b.text().len());
        b.undo();
        assert_eq!(b.text(), "");
        b.undo(); // nothing left
        assert_eq!(b.text(), "");
        b.redo();
        assert_eq!(b.text(), "hello world,");
        b.redo();
        assert_eq!(b.text(), "hello world, foo bar");
        assert_eq!(b.cursor(), b.text().len());
    }

    #[test]
    fn inserts_and_newlines_are_single_steps() {
        let mut b = Buffer::default();
        b.insert_str("你好");
        b.insert_str("世界");
        typ(&mut b, "ab");
        b.newline();
        typ(&mut b, "cd");
        for want in ["你好世界ab\n", "你好世界ab", "你好世界", "你好", ""] {
            b.undo();
            assert_eq!(b.text(), want);
        }
    }

    #[test]
    fn motion_ends_the_step() {
        let mut b = Buffer::default();
        typ(&mut b, "abc");
        b.left();
        b.right();
        typ(&mut b, "d");
        b.undo();
        assert_eq!(b.text(), "abc");
    }

    #[test]
    fn deletions_merge_by_kind() {
        let mut b = buf("abcdef");
        b.left();
        b.left();
        b.left();
        b.backspace();
        b.backspace();
        b.delete();
        b.delete();
        assert_eq!(b.text(), "af");
        b.undo();
        assert_eq!(b.text(), "adef");
        assert_eq!(b.cursor(), 1, "back to where the deletes started");
        b.undo();
        assert_eq!(b.text(), "abcdef");
        assert_eq!(b.cursor(), 3, "back to where the backspaces started");
        b.backspace();
        b.backspace();
        b.right();
        b.backspace(); // after a motion: a new step
        assert_eq!(b.text(), "aef");
        b.undo();
        assert_eq!(b.text(), "adef");
    }

    #[test]
    fn kills_and_clear_are_undoable() {
        let mut b = buf("ab cd\nef");
        b.kill_word();
        b.buffer_start();
        b.kill_to_end();
        b.clear();
        for want in ["\n", "ab cd\n", "ab cd\nef"] {
            b.undo();
            assert_eq!(b.text(), want);
        }
        // Undoing the kill restores the cursor from before it.
        assert_eq!(b.cursor(), b.text().len());
    }

    #[test]
    fn new_edit_drops_redo() {
        let mut b = Buffer::default();
        typ(&mut b, "ab");
        b.undo();
        typ(&mut b, "c");
        b.redo();
        assert_eq!(b.text(), "c");
        b.undo();
        assert_eq!(b.text(), "");
    }

    #[test]
    fn no_op_edits_are_not_recorded() {
        let mut b = buf("a");
        b.buffer_start();
        b.backspace();
        b.kill_to_start();
        b.buffer_end();
        b.delete();
        b.undo();
        assert_eq!(b.text(), "");
    }

    #[test]
    fn undo_history_is_bounded() {
        let mut b = Buffer::default();
        for _ in 0..UNDO_LIMIT + 5 {
            b.insert_str("x");
        }
        for _ in 0..UNDO_LIMIT + 5 {
            b.undo();
        }
        assert_eq!(b.text(), "x".repeat(5));
    }

    #[test]
    fn set_text_starts_a_new_history() {
        let mut b = buf("old");
        b.set_text("new");
        b.undo();
        assert_eq!(b.text(), "new");
    }
}
