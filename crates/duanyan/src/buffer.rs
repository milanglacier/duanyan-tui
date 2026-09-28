//! The multi-line input buffer holding committed text.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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
        self.text[self.line_start()..self.cursor].width()
    }

    /// Byte offset in the line starting at `start` closest to display column
    /// `col` without passing it.
    fn offset_at_col(&self, start: usize, col: usize) -> usize {
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |i| start + i);
        let mut w = 0;
        for (i, g) in self.text[start..end].grapheme_indices(true) {
            let gw = g.width();
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

    /// Deletes the word before the cursor: trailing blanks plus one word
    /// segment (UAX #29, so each CJK ideograph is its own word). A line break
    /// directly before the cursor is deleted on its own.
    pub fn kill_word(&mut self) {
        let before = &self.text[..self.cursor];
        let start = if before.ends_with('\n') {
            self.cursor - 1
        } else {
            let segs: Vec<(usize, &str)> = before.split_word_bound_indices().collect();
            let mut i = segs.len();
            while i > 0
                && segs[i - 1]
                    .1
                    .chars()
                    .all(|c| c.is_whitespace() && c != '\n')
            {
                i -= 1;
            }
            if i > 0 && !segs[i - 1].1.contains('\n') {
                i -= 1;
            }
            segs.get(i).map_or(self.cursor, |(off, _)| *off)
        };
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
    fn take_resets() {
        let mut b = buf("x\ny");
        assert_eq!(b.take(), "x\ny");
        assert!(b.is_empty());
        assert_eq!(b.cursor(), 0);
    }
}
