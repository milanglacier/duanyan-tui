//! The multi-line input buffer holding committed text.

use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const TAB_STOP: usize = 8;

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

#[derive(Debug, Default, Clone)]
pub struct Buffer {
    text: String,
    /// Byte offset, always on a grapheme boundary.
    cursor: usize,
    /// Display column kept across vertical moves.
    goal_col: Option<usize>,
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

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.goal_col = None;
    }

    /// Replaces the text; the cursor goes to the start.
    pub fn set_text(&mut self, text: &str) {
        self.clear();
        self.text.push_str(text);
    }

    /// Takes the text and resets the buffer.
    pub fn take(&mut self) -> String {
        let t = std::mem::take(&mut self.text);
        self.clear();
        t
    }

    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
        self.goal_col = None;
    }

    pub fn newline(&mut self) {
        self.insert_str("\n");
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
        if let Some(i) = self.prev_boundary() {
            self.cursor = i;
        }
        self.goal_col = None;
    }

    pub fn right(&mut self) {
        if let Some(i) = self.next_boundary() {
            self.cursor = i;
        }
        self.goal_col = None;
    }

    pub fn home(&mut self) {
        self.cursor = self.line_start();
        self.goal_col = None;
    }

    pub fn end(&mut self) {
        self.cursor = self.line_end();
        self.goal_col = None;
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
    }

    pub fn next_line(&mut self) {
        let end = self.line_end();
        if end == self.text.len() {
            return;
        }
        let goal = *self.goal_col.get_or_insert(self.col());
        self.cursor = self.offset_at_col(end + 1, goal);
    }

    pub fn backspace(&mut self) {
        if let Some(i) = self.prev_boundary() {
            self.text.replace_range(i..self.cursor, "");
            self.cursor = i;
        }
        self.goal_col = None;
    }

    pub fn delete(&mut self) {
        if let Some(i) = self.next_boundary() {
            self.text.replace_range(self.cursor..i, "");
        }
        self.goal_col = None;
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
        self.cursor = self.word_start_before();
        self.goal_col = None;
    }

    pub fn word_right(&mut self) {
        self.cursor = self.word_end_after();
        self.goal_col = None;
    }

    pub fn buffer_start(&mut self) {
        self.cursor = 0;
        self.goal_col = None;
    }

    pub fn buffer_end(&mut self) {
        self.cursor = self.text.len();
        self.goal_col = None;
    }

    /// Moves the cursor to byte offset `at`, which must be a grapheme
    /// boundary.
    pub fn set_cursor(&mut self, at: usize) {
        debug_assert!(self.text.is_char_boundary(at));
        self.cursor = at.min(self.text.len());
        self.goal_col = None;
    }

    /// Deletes `range`, which must lie on grapheme boundaries; the cursor
    /// goes to its start.
    pub fn delete_range(&mut self, range: std::ops::Range<usize>) {
        self.cursor = range.start;
        self.text.replace_range(range, "");
        self.goal_col = None;
    }

    /// Deletes the word before the cursor (see [`Self::word_start_before`]).
    pub fn kill_word(&mut self) {
        let start = self.word_start_before();
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
        self.goal_col = None;
    }

    /// Deletes to the start of the line; at the start, joins with the
    /// previous line.
    pub fn kill_to_start(&mut self) {
        let start = self.line_start();
        if start == self.cursor {
            self.backspace();
        } else {
            self.text.replace_range(start..self.cursor, "");
            self.cursor = start;
        }
        self.goal_col = None;
    }

    /// Deletes to the end of the line; at the end, deletes the line break.
    pub fn kill_to_end(&mut self) {
        let end = self.line_end();
        if end == self.cursor {
            self.delete();
        } else {
            self.text.replace_range(self.cursor..end, "");
        }
        self.goal_col = None;
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
    }
}
