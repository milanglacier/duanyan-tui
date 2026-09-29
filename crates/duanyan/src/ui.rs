//! Rendering. `draw` returns the clickable regions it laid out.

use std::ops::Range;

use jiff::tz::TimeZone;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, CellHit, Focus, HitMap, Level, Mode, RowHit, TextHits};
use crate::buffer::cell;
use crate::config::{CandidateLayout, GlobalAction, HistoryAction, InputAction};
use crate::engine::{Candidate, ImeEngine, ImeSnapshot, Maintenance, Preedit};
use crate::keys::KeySpec;
use crate::theme::Theme;

/// Static facts shown in the UI.
pub struct UiContext {
    pub inline: bool,
    pub theme: Theme,
    pub candidate_layout: CandidateLayout,
    pub show_comment: bool,
    /// The file being edited, shown in the header.
    pub edit_path: Option<String>,
    /// (label, value) pairs for the help screen.
    pub info: Vec<(String, String)>,
}

/// Text rows the inline viewport reserves; longer buffers scroll.
pub const INLINE_TEXT_ROWS: u16 = 3;

pub fn inline_height(ctx: &UiContext, page_size: usize) -> u16 {
    INLINE_TEXT_ROWS + candidate_rows(ctx, page_size) + 1
}

fn candidate_rows(ctx: &UiContext, n: usize) -> u16 {
    match ctx.candidate_layout {
        CandidateLayout::Horizontal => 1,
        CandidateLayout::Vertical => n.clamp(1, 10) as u16,
    }
}

pub fn draw<E: ImeEngine>(f: &mut Frame, app: &App<E>, ctx: &UiContext) -> HitMap {
    let mut hits = HitMap::default();
    let t = &ctx.theme;
    let area = f.area();
    if area.width < 4 || area.height < 3 {
        return hits;
    }
    if ctx.inline {
        draw_inline(f, app, ctx, area, &mut hits);
    } else {
        f.render_widget(
            Block::default().style(Style::default().bg(t.bg).fg(t.text)),
            area,
        );
        draw_fullscreen(f, app, ctx, area, &mut hits);
    }
    if app.show_help {
        draw_help(f, app, ctx, area);
    }
    hits
}

fn draw_fullscreen<E: ImeEngine>(
    f: &mut Frame,
    app: &App<E>,
    ctx: &UiContext,
    area: Rect,
    hits: &mut HitMap,
) {
    let t = &ctx.theme;
    let inner = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height,
    );
    let header = Rect::new(inner.x, inner.y, inner.width, 1);
    let status = Rect::new(area.x, area.bottom() - 1, area.width, 1);

    // Header.
    let mut spans = vec![Span::styled(
        "端砚-tui",
        Style::default().fg(t.title).add_modifier(Modifier::BOLD),
    )];
    if !app.snapshot.schema_name.is_empty() {
        spans.push(Span::styled(
            format!("  {}", app.snapshot.schema_name),
            Style::default().fg(t.subtext),
        ));
    }
    let version = concat!("v", env!("CARGO_PKG_VERSION"));
    if let Some(path) = &ctx.edit_path {
        // Keep the file name: callers such as git pass long absolute paths.
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        let room = (header.width as usize).saturating_sub(used + version.width() + 4);
        spans.push(Span::styled(
            format!("  {}", truncate_start(path, room)),
            Style::default().fg(t.text),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), header);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            version,
            Style::default().fg(t.subtext),
        )))
        .right_aligned(),
        header,
    );

    // Input block, sized to its content; editing a file, it fills the body
    // and the history is hidden.
    let edit = app.mode == Mode::Edit;
    let text_width = inner.width.saturating_sub(4).max(1);
    let layout = layout_text(app, t, text_width);
    let max_text_rows = (area.height / 2).max(1);
    let text_rows = (layout.rows.len() as u16).clamp(1, max_text_rows);
    let cand_rows = candidate_rows(ctx, app.snapshot.candidates.len());
    let body_top = header.bottom();
    let body_bottom = status.y;
    let body_h = body_bottom.saturating_sub(body_top);
    let input_h = if edit {
        body_h
    } else {
        (text_rows + cand_rows + 2).min(body_h)
    };
    let input = Rect::new(inner.x, body_bottom - input_h, inner.width, input_h);
    let history = Rect::new(
        inner.x,
        body_top,
        inner.width,
        input.y.saturating_sub(body_top),
    );

    if !edit && history.height >= 3 {
        draw_history(f, app, ctx, history, hits);
    }
    let focused = app.focus == Focus::Input;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { t.border_focus } else { t.border }))
        .title(Span::styled(
            " 输入 ",
            Style::default().fg(if focused { t.accent } else { t.subtext }),
        ));
    let content = block.inner(input);
    f.render_widget(block, input);
    hits.input_area = Some(input);
    if content.height == 0 {
        return;
    }
    // Editing a file, the candidates float over the text instead.
    let text_rows = if edit { content.height } else { text_rows };
    let text_area = Rect::new(
        content.x + 1,
        content.y,
        content.width.saturating_sub(2),
        text_rows.min(content.height),
    );
    let offset = render_text(f, app, t, &layout, text_area, focused && !app.show_help);
    hits.text_scroll = offset;
    hits.text = Some(TextHits {
        area: Rect::new(content.x, text_area.y, content.width, text_area.height),
        x: text_area.x,
        rows: layout
            .hits
            .iter()
            .skip(offset)
            .take(text_area.height as usize)
            .cloned()
            .collect(),
        end: app.buffer.text().len(),
    });
    if edit {
        // Under the preedit start, unless the preedit wraps.
        let (crow, _) = layout.cursor;
        let col = match layout.preedit_start {
            Some((row, col)) if row == crow => col,
            _ => 0,
        };
        let anchor = Position::new(text_area.x + col, text_area.y + (crow - offset) as u16);
        draw_candidate_popup(f, app, ctx, anchor, content, hits);
    } else {
        let cand_area = Rect::new(
            content.x + 1,
            text_area.bottom(),
            content.width.saturating_sub(2),
            content.bottom().saturating_sub(text_area.bottom()),
        );
        draw_candidates(f, app, ctx, cand_area, hits);
    }
    draw_status(f, app, ctx, status, hits);
}

fn draw_inline<E: ImeEngine>(
    f: &mut Frame,
    app: &App<E>,
    ctx: &UiContext,
    area: Rect,
    hits: &mut HitMap,
) {
    let t = &ctx.theme;
    let width = area.width.saturating_sub(2).max(1);
    let layout = layout_text(app, t, width);
    // Candidates follow the text directly; spare rows stay blank above
    // the status line.
    let text_rows = (layout.rows.len() as u16).clamp(1, INLINE_TEXT_ROWS);
    let text_area = Rect::new(area.x + 1, area.y, width, text_rows.min(area.height));
    hits.text_scroll = render_text(f, app, t, &layout, text_area, !app.show_help);
    let status = Rect::new(area.x, area.bottom() - 1, area.width, 1);
    let cand_area = Rect::new(
        area.x + 1,
        text_area.bottom(),
        width,
        status.y.saturating_sub(text_area.bottom()),
    );
    hits.input_area = Some(area);
    draw_candidates(f, app, ctx, cand_area, hits);
    draw_status(f, app, ctx, status, hits);
}

/// A laid-out (wrapped) view of buffer + preedit.
struct TextLayout {
    rows: Vec<Vec<(String, Style)>>,
    /// Per row, where clicks put the cursor.
    hits: Vec<RowHit>,
    cursor: (usize, u16),
    /// Where the preedit starts, if there is one.
    preedit_start: Option<(usize, u16)>,
}

fn layout_text<E: ImeEngine>(app: &App<E>, t: &Theme, width: u16) -> TextLayout {
    layout_cells(
        app.buffer.text(),
        app.buffer.cursor(),
        app.selection.clone(),
        app.snapshot.preedit.as_ref(),
        t,
        width,
    )
}

/// A grapheme to lay out, with the buffer offsets before and after it.
struct Cell<'a> {
    g: &'a str,
    style: Style,
    before: usize,
    after: usize,
}

fn layout_cells(
    text: &str,
    cursor: usize,
    selection: Option<Range<usize>>,
    preedit: Option<&Preedit>,
    t: &Theme,
    width: u16,
) -> TextLayout {
    let normal = Style::default().fg(t.text);
    let selected = normal.bg(t.selection_bg);
    let style_at = |off: usize| match &selection {
        Some(r) if r.contains(&off) => selected,
        _ => normal,
    };
    let mut cells: Vec<Cell> = Vec::new();
    push_text(&mut cells, text, 0..cursor, style_at);
    let mut cursor_cell = cells.len();
    let mut preedit_cell = None;
    if let Some(p) = preedit.filter(|p| !p.text.is_empty()) {
        preedit_cell = Some(cells.len());
        cursor_cell = push_preedit(&mut cells, p, cursor, t);
    }
    push_text(&mut cells, text, cursor..text.len(), style_at);

    let width = width as usize;
    let mut rows: Vec<Vec<(String, Style)>> = vec![Vec::new()];
    let mut hits = vec![RowHit::default()];
    let mut col = 0usize;
    let mut cursor = (0, 0);
    let mut preedit_start = None;
    for (i, c) in cells.iter().enumerate() {
        if i == cursor_cell {
            cursor = (rows.len() - 1, col as u16);
        }
        if Some(i) == preedit_cell {
            preedit_start = Some((rows.len() - 1, col as u16));
        }
        if c.g == "\n" {
            hits.last_mut().unwrap().end = c.before;
            rows.push(Vec::new());
            hits.push(RowHit::default());
            col = 0;
            continue;
        }
        let mut shown = cell(c.g, col);
        if col + shown.1 > width && col > 0 {
            hits.last_mut().unwrap().end = c.before;
            rows.push(Vec::new());
            hits.push(RowHit::default());
            col = 0;
            if i == cursor_cell {
                cursor = (rows.len() - 1, 0);
            }
            if Some(i) == preedit_cell {
                preedit_start = Some((rows.len() - 1, 0));
            }
            shown = cell(c.g, 0);
        }
        let (s, w) = shown;
        let row = rows.last_mut().unwrap();
        match row.last_mut() {
            Some((text, st)) if *st == c.style => text.push_str(&s),
            _ => row.push((s.into_owned(), c.style)),
        }
        hits.last_mut().unwrap().cells.push(CellHit {
            col: col as u16,
            width: w as u16,
            before: c.before,
            after: c.after,
        });
        col += w;
    }
    hits.last_mut().unwrap().end = text.len();
    if cursor_cell == cells.len() {
        if col >= width {
            rows.push(Vec::new());
            hits.push(RowHit {
                cells: Vec::new(),
                end: text.len(),
            });
            col = 0;
        }
        cursor = (rows.len() - 1, col as u16);
    }
    TextLayout {
        rows,
        hits,
        cursor,
        preedit_start,
    }
}

/// Pushes one cell per grapheme of `text[range]`. A CRLF is one grapheme but
/// two cells, so the `\n` still ends the row; both cells cover the whole
/// grapheme.
fn push_text<'a>(
    cells: &mut Vec<Cell<'a>>,
    text: &'a str,
    range: Range<usize>,
    style_at: impl Fn(usize) -> Style,
) {
    let base = range.start;
    for (i, g) in text[range].grapheme_indices(true) {
        let (before, after) = (base + i, base + i + g.len());
        let style = style_at(before);
        let parts = if g == "\r\n" {
            vec![&g[..1], &g[1..]]
        } else {
            vec![g]
        };
        for g in parts {
            cells.push(Cell {
                g,
                style,
                before,
                after,
            });
        }
    }
}

/// Pushes preedit cells, which sit at buffer offset `at`; returns the cell
/// index of rime's caret.
fn push_preedit<'a>(cells: &mut Vec<Cell<'a>>, p: &'a Preedit, at: usize, t: &Theme) -> usize {
    let converted = Style::default().fg(t.preedit);
    let active = Style::default()
        .fg(t.text)
        .add_modifier(Modifier::UNDERLINED);
    let rest = Style::default().fg(t.subtext);
    let start = cells.len();
    let mut caret = None;
    for (off, g) in p.text.grapheme_indices(true) {
        if caret.is_none() && off >= p.cursor {
            caret = Some(cells.len());
        }
        let style = if off < p.sel_start {
            converted
        } else if off < p.sel_end {
            active
        } else {
            rest
        };
        cells.push(Cell {
            g,
            style,
            before: at,
            after: at,
        });
    }
    caret.unwrap_or(if p.cursor == 0 { start } else { cells.len() })
}

/// First row to show: the previous one, unless that leaves blank rows at the
/// bottom or the cursor row out of view.
fn scroll_offset(prev: usize, rows: usize, cursor_row: usize, height: usize) -> usize {
    prev.min(rows.saturating_sub(height))
        .clamp((cursor_row + 1).saturating_sub(height), cursor_row)
}

/// Draws the text rows; returns the first row shown.
fn render_text<E: ImeEngine>(
    f: &mut Frame,
    app: &App<E>,
    t: &Theme,
    layout: &TextLayout,
    area: Rect,
    show_cursor: bool,
) -> usize {
    if area.height == 0 || area.width == 0 {
        return 0;
    }
    let (crow, ccol) = layout.cursor;
    let offset = scroll_offset(
        app.hits.text_scroll,
        layout.rows.len(),
        crow,
        area.height as usize,
    );
    let lines: Vec<Line> = layout
        .rows
        .iter()
        .skip(offset)
        .take(area.height as usize)
        .map(|r| {
            Line::from(
                r.iter()
                    .map(|(s, st)| Span::styled(s.clone(), *st))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    f.render_widget(Paragraph::new(lines), area);
    let cursor_y = area.y + (crow - offset) as u16;

    // Raw input, right-aligned on the cursor row when it fits.
    if let Some(raw) = &app.snapshot.raw_input {
        let used: usize = layout.rows[crow].iter().map(|(s, _)| s.width()).sum();
        let rw = raw.width();
        if used + rw + 2 <= area.width as usize {
            let r = Rect::new(area.right() - rw as u16, cursor_y, rw as u16, 1);
            f.render_widget(
                Paragraph::new(Span::styled(raw.clone(), Style::default().fg(t.subtext))),
                r,
            );
        }
    }
    if show_cursor {
        f.set_cursor_position(Position::new(
            (area.x + ccol).min(area.right().saturating_sub(1)),
            cursor_y,
        ));
    }
    offset
}

/// Editing a file: the candidates in a box placed by `popup_rect` around
/// `anchor`, the screen cell of the preedit start.
fn draw_candidate_popup<E: ImeEngine>(
    f: &mut Frame,
    app: &App<E>,
    ctx: &UiContext,
    anchor: Position,
    bounds: Rect,
    hits: &mut HitMap,
) {
    if app.snapshot.candidates.is_empty() {
        return;
    }
    let (w, h) = candidates_size(ctx, &app.snapshot);
    // Borders on both sides and a column of padding on the right; each label
    // already starts with a space.
    let Some(rect) = popup_rect(anchor, (w + 3, h + 2), bounds) else {
        return;
    };
    let t = &ctx.theme;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.bg));
    let inner = block.inner(rect);
    f.render_widget(Clear, rect);
    f.render_widget(block, rect);
    let inner = Rect::new(
        inner.x,
        inner.y,
        inner.width.saturating_sub(1),
        inner.height,
    );
    draw_candidates(f, app, ctx, inner, hits);
}

/// Places a box of `size` below the row of `anchor`, starting a column left
/// of it, inside `bounds`. It moves left to fit, goes above the row when only
/// that side has room, and otherwise shrinks to the larger side; `None` when
/// fewer than 3 rows (one inside the borders) remain.
fn popup_rect(anchor: Position, size: (u16, u16), bounds: Rect) -> Option<Rect> {
    let w = size.0.min(bounds.width);
    let x = anchor
        .x
        .saturating_sub(1)
        .max(bounds.x)
        .min(bounds.right() - w);
    let below = bounds.bottom().saturating_sub(anchor.y + 1);
    let above = anchor.y.saturating_sub(bounds.y);
    let (y, h) = if size.1 <= below || below >= above {
        (anchor.y + 1, size.1.min(below))
    } else {
        let h = size.1.min(above);
        (anchor.y - h, h)
    };
    (h >= 3).then(|| Rect::new(x, y, w, h))
}

fn candidate_spans<'a>(ctx: &UiContext, c: &'a Candidate, highlighted: bool) -> Vec<Span<'a>> {
    let t = &ctx.theme;
    let hl = Style::default()
        .bg(t.candidate_hl_bg)
        .fg(t.candidate_hl_fg)
        .add_modifier(Modifier::BOLD);
    let idx = Style::default().fg(t.candidate_index);
    let text = Style::default().fg(t.text);
    let mut spans = vec![
        Span::styled(format!(" {} ", c.label), if highlighted { hl } else { idx }),
        Span::styled(format!("{} ", c.text), if highlighted { hl } else { text }),
    ];
    if ctx.show_comment
        && let Some(cm) = &c.comment
    {
        spans.push(Span::styled(
            format!("{cm} "),
            Style::default().fg(t.comment),
        ));
    }
    spans
}

fn spans_width(spans: &[Span]) -> u16 {
    spans.iter().map(|s| s.content.width() as u16).sum()
}

/// The (width, height) `draw_candidates` needs to show the whole page.
fn candidates_size(ctx: &UiContext, snap: &ImeSnapshot) -> (u16, u16) {
    let widths = snap
        .candidates
        .iter()
        .map(|c| spans_width(&candidate_spans(ctx, c, false)));
    let ind_w = page_indicator_width(snap);
    match ctx.candidate_layout {
        CandidateLayout::Horizontal => {
            let n = snap.candidates.len() as u16;
            (widths.sum::<u16>() + n.saturating_sub(1) + ind_w + 1, 1)
        }
        CandidateLayout::Vertical => (
            widths.max().unwrap_or(0) + ind_w + 1,
            candidate_rows(ctx, snap.candidates.len()),
        ),
    }
}

/// Width of the ‹ n › page indicator.
fn page_indicator_width(snap: &ImeSnapshot) -> u16 {
    format!(" {} ", snap.page_no + 1).width() as u16 + 2
}

fn draw_candidates<E: ImeEngine>(
    f: &mut Frame,
    app: &App<E>,
    ctx: &UiContext,
    area: Rect,
    hits: &mut HitMap,
) {
    let t = &ctx.theme;
    let snap = &app.snapshot;
    if area.height == 0 || area.width == 0 || snap.candidates.is_empty() {
        return;
    }
    // Page indicator: ‹ n ›, arrows dimmed at the ends.
    let page = format!(" {} ", snap.page_no + 1);
    let dim = Style::default().fg(t.border);
    let on = Style::default().fg(t.subtext);
    let ind_w = page_indicator_width(snap);
    let ind = Rect::new(area.right().saturating_sub(ind_w), area.y, ind_w, 1);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("‹", if snap.page_no > 0 { on } else { dim }),
            Span::styled(page, on),
            Span::styled("›", if snap.is_last_page { dim } else { on }),
        ])),
        ind,
    );
    if snap.page_no > 0 {
        hits.prev_page = Some(Rect::new(ind.x, ind.y, 1, 1));
    }
    if !snap.is_last_page {
        hits.next_page = Some(Rect::new(ind.right() - 1, ind.y, 1, 1));
    }
    let avail = area.width.saturating_sub(ind_w + 1);

    let vertical = ctx.candidate_layout == CandidateLayout::Vertical;
    let mut x = area.x;
    let mut y = area.y;
    for (i, c) in snap.candidates.iter().enumerate() {
        let spans = candidate_spans(ctx, c, i == snap.highlighted);
        let w = spans_width(&spans);
        let row_limit = if vertical && y > area.y {
            area.width
        } else {
            avail
        };
        if vertical {
            if y >= area.bottom() {
                break;
            }
            x = area.x;
        } else if x + w > area.x + row_limit {
            break;
        }
        let w = w.min(row_limit);
        let r = Rect::new(x, y, w, 1);
        f.render_widget(Paragraph::new(Line::from(spans)), r);
        hits.candidates.push((r, i));
        if vertical {
            y += 1;
        } else {
            x += w + 1;
        }
    }
}

fn key_label(app_keys: &[KeySpec]) -> Option<String> {
    app_keys.first().map(|k| {
        let s = k.to_string();
        // Title-case the common names for the hint bar.
        match s.as_str() {
            "tab" => "Tab".into(),
            "enter" => "Enter".into(),
            "esc" => "Esc".into(),
            _ if s.starts_with('f') && s[1..].parse::<u8>().is_ok() => s.to_uppercase(),
            _ => s,
        }
    })
}

fn draw_status<E: ImeEngine>(
    f: &mut Frame,
    app: &App<E>,
    ctx: &UiContext,
    area: Rect,
    hits: &mut HitMap,
) {
    let t = &ctx.theme;
    let base = if ctx.inline {
        Style::default()
    } else {
        Style::default().bg(t.status_bg)
    };
    f.render_widget(Block::default().style(base), area);
    let mut x = area.x + if ctx.inline { 1 } else { 0 };
    let right_edge = area.right();
    let put = |f: &mut Frame, text: String, style: Style, x: &mut u16| -> Option<Rect> {
        let w = text.width() as u16;
        if *x + w > right_edge {
            return None;
        }
        let r = Rect::new(*x, area.y, w, 1);
        f.render_widget(Paragraph::new(Span::styled(text, style)), r);
        *x += w;
        Some(r)
    };

    let sep = Style::default().fg(t.border);
    let label = Style::default().fg(t.text);
    for (i, sw) in app.snapshot.switches.iter().enumerate() {
        let style = if sw.is_ascii_mode {
            Style::default()
                .bg(t.accent)
                .fg(t.candidate_hl_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            label
        };
        if i > 0 && !app.snapshot.switches[i - 1].is_ascii_mode {
            put(f, "│".into(), sep, &mut x);
        }
        if let Some(r) = put(f, format!(" {} ", sw.label), style, &mut x) {
            hits.switches.push((r, i));
        }
        if sw.is_ascii_mode {
            put(f, " ".into(), label, &mut x);
        }
    }

    // Most recent first: a transient message, then persistent states.
    let mut notes: Vec<(String, Style)> = Vec::new();
    if let Some(m) = &app.message {
        let color = match m.level {
            Level::Info => t.text,
            Level::Success => t.success,
            Level::Warning => t.warning,
            Level::Error => t.error,
        };
        notes.push((m.text.clone(), Style::default().fg(color)));
    }
    match app.engine.busy() {
        Some(Maintenance::Deploy) => {
            notes.push(("⟳ 部署中…".into(), Style::default().fg(t.warning)))
        }
        Some(Maintenance::Sync) => notes.push(("⟳ 同步中…".into(), Style::default().fg(t.warning))),
        None => {}
    }
    if app.deploy_hint && app.engine.busy().is_none() {
        let key = key_label(&app.keymap.keys_for("global", GlobalAction::Deploy.name()))
            .unwrap_or_default();
        notes.push((
            format!("⚠ 配置已变更 · {key} 部署"),
            Style::default().fg(t.warning),
        ));
    }
    if app.secondary {
        notes.push((
            "⚠ 已有端砚在运行，不学习新词".into(),
            Style::default().fg(t.warning),
        ));
    }
    for (text, style) in notes {
        put(f, format!("  {text}"), style, &mut x);
    }

    // Key hints on the right, dropped when they do not fit.
    let km = &app.keymap;
    let hint = |table: &str, action: &str, what: &str| {
        key_label(&km.keys_for(table, action)).map(|k| (k, what.to_string()))
    };
    let hints: Vec<(String, String)> = if app.mode == Mode::Stdout {
        [
            hint("input", InputAction::Submit.name(), "输出"),
            hint("input", InputAction::Cancel.name(), "取消"),
            hint("global", GlobalAction::Help.name(), "帮助"),
        ]
        .into_iter()
        .flatten()
        .collect()
    } else if app.mode == Mode::Edit {
        [
            hint("input", InputAction::Submit.name(), "保存"),
            hint("input", InputAction::Cancel.name(), "放弃"),
            hint("input", InputAction::Newline.name(), "换行"),
            hint("global", GlobalAction::Help.name(), "帮助"),
        ]
        .into_iter()
        .flatten()
        .collect()
    } else if app.focus == Focus::History {
        [
            hint("history", HistoryAction::Copy.name(), "复制"),
            hint("history", HistoryAction::Recall.name(), "取回"),
            hint("history", HistoryAction::Delete.name(), "删除"),
            hint("history", HistoryAction::FocusInput.name(), "输入"),
            hint("global", GlobalAction::Help.name(), "帮助"),
        ]
        .into_iter()
        .flatten()
        .collect()
    } else {
        [
            hint("input", InputAction::FocusHistory.name(), "历史"),
            hint("input", InputAction::Newline.name(), "换行"),
            hint("global", GlobalAction::Help.name(), "帮助"),
        ]
        .into_iter()
        .flatten()
        .collect()
    };
    let mut spans = Vec::new();
    for (k, what) in &hints {
        spans.push(Span::styled(
            format!(" {k} "),
            Style::default().bg(t.key_bg).fg(t.text),
        ));
        spans.push(Span::styled(
            format!(" {what}  "),
            Style::default().fg(t.subtext),
        ));
    }
    let w: u16 = spans.iter().map(|s| s.content.width() as u16).sum();
    if x + w < right_edge {
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(right_edge - w, area.y, w, 1),
        );
    }
}

fn format_time(ts: i64, tz: &TimeZone, today: jiff::civil::Date) -> String {
    match jiff::Timestamp::from_second(ts) {
        Ok(t) => {
            let z = t.to_zoned(tz.clone());
            if z.date() == today {
                z.strftime("%H:%M").to_string()
            } else {
                z.strftime("%m-%d %H:%M").to_string()
            }
        }
        Err(_) => "--:--".into(),
    }
}

/// Truncates to `width` display columns, appending `…` when cut.
fn truncate(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for g in s.graphemes(true) {
        let gw = g.width();
        if w + gw + 1 > width {
            break;
        }
        out.push_str(g);
        w += gw;
    }
    out.push('…');
    out
}

/// Truncates to `width` display columns, keeping the end behind a `…`.
fn truncate_start(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    let mut kept = Vec::new();
    let mut w = 0;
    for g in s.graphemes(true).rev() {
        let gw = g.width();
        if w + gw + 1 > width {
            break;
        }
        kept.push(g);
        w += gw;
    }
    std::iter::once("…").chain(kept.into_iter().rev()).collect()
}

fn draw_history<E: ImeEngine>(
    f: &mut Frame,
    app: &App<E>,
    ctx: &UiContext,
    area: Rect,
    hits: &mut HitMap,
) {
    let t = &ctx.theme;
    let focused = app.focus == Focus::History;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { t.border_focus } else { t.border }))
        .title(Span::styled(
            format!(" 历史 · {} 条 ", app.history.len()),
            Style::default().fg(if focused { t.accent } else { t.subtext }),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    hits.history_area = Some(area);
    if inner.height == 0 || inner.width < 8 {
        return;
    }
    let entries = app.history.entries();
    if entries.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "提交的文字会出现在这里",
                Style::default().fg(t.subtext),
            ))
            .centered(),
            Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1),
        );
        return;
    }

    let tz = TimeZone::system();
    let today = jiff::Zoned::now().date();
    let copied = app.copied.map(|(i, _)| i);
    let marker = "✓ 已复制到剪贴板";
    let max_expand = (inner.height as usize / 2).max(1);

    // Rows: (entry index, first row of entry?, spans).
    let mut rows: Vec<(usize, Vec<Span>)> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let time = format_time(e.ts, &tz, today);
        let prefix_w = time.width() + 3;
        let mut text_w = (inner.width as usize).saturating_sub(prefix_w + 1);
        if copied == Some(i) {
            text_w = text_w.saturating_sub(marker.width() + 2);
        }
        let selected = focused && i == app.history_sel;
        let time_span = Span::styled(format!(" {time}  "), Style::default().fg(t.subtext));
        if selected && e.text.contains('\n') {
            for (j, line) in e.text.split('\n').take(max_expand).enumerate() {
                let lead = if j == 0 {
                    time_span.clone()
                } else {
                    Span::raw(" ".repeat(prefix_w))
                };
                rows.push((
                    i,
                    vec![
                        lead,
                        Span::styled(truncate(line, text_w), Style::default().fg(t.text)),
                    ],
                ));
            }
        } else {
            let mut spans = vec![time_span];
            let mut w = 0;
            for (j, part) in e.text.split('\n').enumerate() {
                if j > 0 {
                    spans.push(Span::styled("↵", Style::default().fg(t.subtext)));
                    w += 1;
                }
                let part = truncate(part, text_w.saturating_sub(w));
                w += part.width();
                spans.push(Span::styled(part, Style::default().fg(t.text)));
                if w >= text_w {
                    break;
                }
            }
            rows.push((i, spans));
        }
    }

    // Window: keep the selection visible, otherwise show the newest rows.
    let h = inner.height as usize;
    let anchor = if focused {
        rows.iter()
            .rposition(|(i, _)| *i == app.history_sel)
            .unwrap_or(rows.len() - 1)
    } else {
        rows.len() - 1
    };
    let start = (anchor + 1).saturating_sub(h);
    for (k, (i, spans)) in rows.iter().enumerate().skip(start).take(h) {
        let y = inner.y + (k - start) as u16;
        let r = Rect::new(inner.x, y, inner.width, 1);
        let selected = focused && *i == app.history_sel;
        let mut spans = spans.clone();
        let style = if selected {
            // Replace the leading space with a bar, keeping the indent.
            let lead: String = spans[0].content.chars().skip(1).collect();
            spans[0] = Span::styled(format!("▎{lead}"), Style::default().fg(t.accent));
            Style::default().bg(t.selection_bg)
        } else {
            Style::default()
        };
        f.render_widget(Paragraph::new(Line::from(spans)).style(style), r);
        let first_row = k == 0 || rows[k - 1].0 != *i;
        if copied == Some(*i) && first_row {
            let mw = marker.width() as u16 + 1;
            f.render_widget(
                Paragraph::new(Span::styled(marker, Style::default().fg(t.success))),
                Rect::new(r.right().saturating_sub(mw), y, mw, 1),
            );
        }
        hits.history_rows.push((r, *i));
    }
}

fn action_label(table: &str, action: &str) -> &'static str {
    match (table, action) {
        ("global", "quit") => "退出",
        ("global", "help") => "帮助",
        ("global", "deploy") => "重新部署 rime",
        ("global", "sync") => "同步用户词典",
        ("input", "submit") => "提交 / 保存",
        ("input", "newline") => "换行",
        ("input", "focus_history") => "切到历史",
        ("input", "cancel") => "取消 / 清空 / 放弃",
        ("input", "left") => "左移",
        ("input", "right") => "右移",
        ("input", "word_left") => "前一词",
        ("input", "word_right") => "后一词",
        ("input", "prev_line") => "上一行",
        ("input", "next_line") => "下一行",
        ("input", "home") => "行首",
        ("input", "end") => "行尾",
        ("input", "buffer_start") => "文本开头",
        ("input", "buffer_end") => "文本结尾",
        ("input", "backspace") => "删除前一字",
        ("input", "delete") => "删除后一字",
        ("input", "kill_word") => "删除前一词",
        ("input", "kill_to_start") => "删到行首",
        ("input", "kill_to_end") => "删到行尾",
        ("history", "next") => "下一条",
        ("history", "prev") => "上一条",
        ("history", "first") => "第一条",
        ("history", "last") => "最后一条",
        ("history", "copy") => "复制",
        ("history", "recall") => "取回编辑",
        ("history", "delete") => "删除",
        ("history", "focus_input") => "返回输入",
        _ => "",
    }
}

fn draw_help<E: ImeEngine>(f: &mut Frame, app: &App<E>, ctx: &UiContext, area: Rect) {
    let t = &ctx.theme;
    let w = (area.width * 4 / 5).clamp(20.min(area.width), 90);
    let h = area.height.saturating_sub(2).max(3);
    let r = Rect::new(area.x + (area.width - w) / 2, area.y + 1, w, h);
    f.render_widget(Clear, r);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border_focus))
        .style(Style::default().bg(t.bg).fg(t.text))
        .title(Span::styled(
            " 帮助 · j/k 滚动 · 其它键关闭 ",
            Style::default().fg(t.accent),
        ));
    let inner = block.inner(r);
    f.render_widget(block, r);

    let head = Style::default().fg(t.title).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(t.subtext);
    let key_style = Style::default().fg(t.accent);
    let kkp = app.keymap.kkp;
    let mut lines: Vec<Line> = Vec::new();
    for (table, title) in [
        ("global", "全局（不经过 rime）"),
        ("compat", "compat（转换后发给 rime）"),
        ("input", "输入框（rime 未处理时）"),
        ("history", "历史面板"),
    ] {
        lines.push(Line::from(Span::styled(title, head)));
        for (tb, action, keys) in &app.keymap.listing {
            if *tb != table {
                continue;
            }
            let label = if table == "compat" {
                format!("→ {action}")
            } else {
                action_label(table, action).to_string()
            };
            let mut spans = vec![Span::raw(format!("  {}", pad(&label, 18)))];
            if keys.is_empty() {
                spans.push(Span::styled("（未绑定）", dim));
            }
            for (j, k) in keys.iter().enumerate() {
                if j > 0 {
                    spans.push(Span::styled(", ", dim));
                }
                spans.push(Span::styled(k.to_string(), key_style));
                if !kkp && !k.legacy_reachable() {
                    spans.push(Span::styled(
                        "（当前终端不可用）",
                        Style::default().fg(t.warning),
                    ));
                }
            }
            lines.push(Line::from(spans));
        }
        lines.push(Line::default());
    }
    lines.push(Line::from(Span::styled("环境", head)));
    for (k, v) in &ctx.info {
        lines.push(Line::from(vec![
            Span::styled(format!("  {}", pad(k, 18)), dim),
            Span::raw(v.clone()),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "提示：改动 lua/、opencc/ 等子目录不会被自动检测，请手动部署。",
        dim,
    )));
    let max_scroll = (lines.len() as u16).saturating_sub(inner.height);
    f.render_widget(
        Paragraph::new(lines).scroll((app.help_scroll.min(max_scroll), 0)),
        inner,
    );
}

/// Pads to `width` display columns.
fn pad(s: &str, width: usize) -> String {
    let w = s.width();
    format!("{s}{}", " ".repeat(width.saturating_sub(w)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(l: &TextLayout) -> Vec<String> {
        l.rows
            .iter()
            .map(|r| r.iter().map(|(s, _)| s.as_str()).collect())
            .collect()
    }

    #[test]
    fn layout_expands_tabs_and_controls() {
        let text = "#\tab\r\nx\x1b";
        let l = layout_cells(text, 2, None, None, &Theme::MOCHA, 20);
        assert_eq!(rows(&l), ["#       ab^M", "x^["]);
        assert_eq!(l.cursor, (0, 8));
    }

    #[test]
    fn layout_wraps_tab_to_next_row() {
        let l = layout_cells("abcdefg\tx", 9, None, None, &Theme::MOCHA, 10);
        // The tab needs 1 column at 7, fits; "x" is at column 8.
        assert_eq!(rows(&l), ["abcdefg x"]);
        let l = layout_cells("abcdefghi\tx", 11, None, None, &Theme::MOCHA, 10);
        // At column 9 the tab needs 7 columns: it wraps and takes 8.
        assert_eq!(rows(&l), ["abcdefghi", "        x"]);
        assert_eq!(l.cursor, (1, 9));
    }

    /// (col, width, before, after).
    type Cells = Vec<(u16, u16, usize, usize)>;

    /// The cells and the end offset of each row.
    fn hit_rows(l: &TextLayout) -> Vec<(Cells, usize)> {
        l.hits
            .iter()
            .map(|r| {
                let cells = r
                    .cells
                    .iter()
                    .map(|c| (c.col, c.width, c.before, c.after))
                    .collect();
                (cells, r.end)
            })
            .collect()
    }

    #[test]
    fn layout_records_offsets() {
        // "你" is 3 bytes and 2 columns; the tab fills columns 3..8.
        let l = layout_cells("a你\tb\ncd", 0, None, None, &Theme::MOCHA, 20);
        assert_eq!(
            hit_rows(&l),
            [
                (
                    vec![(0, 1, 0, 1), (1, 2, 1, 4), (3, 5, 4, 5), (8, 1, 5, 6)],
                    6
                ),
                (vec![(0, 1, 7, 8), (1, 1, 8, 9)], 9),
            ]
        );
    }

    #[test]
    fn layout_offsets_across_wraps() {
        // A wrapped row ends where the next one starts.
        let l = layout_cells("abcd", 4, None, None, &Theme::MOCHA, 3);
        assert_eq!(hit_rows(&l)[0].1, 3);
        assert_eq!(hit_rows(&l)[1], (vec![(0, 1, 3, 4)], 4));
        // A cursor after a full last row gets a row of its own.
        let l = layout_cells("abc", 3, None, None, &Theme::MOCHA, 3);
        assert_eq!(hit_rows(&l)[0].1, 3);
        assert_eq!(hit_rows(&l)[1], (vec![], 3));
    }

    #[test]
    fn layout_offsets_crlf_and_preedit() {
        // CR and LF form one grapheme.
        let l = layout_cells("a\r\nb", 0, None, None, &Theme::MOCHA, 20);
        assert_eq!(
            hit_rows(&l),
            [
                (vec![(0, 1, 0, 1), (1, 2, 1, 3)], 1),
                (vec![(0, 1, 3, 4)], 4)
            ]
        );
        // Preedit cells sit at the buffer cursor.
        let p = Preedit {
            text: "ni".into(),
            cursor: 2,
            ..Default::default()
        };
        let l = layout_cells("ab", 1, None, Some(&p), &Theme::MOCHA, 20);
        assert_eq!(
            hit_rows(&l),
            [(
                vec![(0, 1, 0, 1), (1, 1, 1, 1), (2, 1, 1, 1), (3, 1, 1, 2)],
                2
            )]
        );
    }

    #[test]
    fn layout_highlights_selection() {
        let t = &Theme::MOCHA;
        let l = layout_cells("a你c", 0, Some(1..4), None, t, 20);
        let normal = Style::default().fg(t.text);
        let row: Vec<(&str, Style)> = l.rows[0].iter().map(|(s, st)| (s.as_str(), *st)).collect();
        assert_eq!(
            row,
            [
                ("a", normal),
                ("你", normal.bg(t.selection_bg)),
                ("c", normal)
            ]
        );
    }

    #[test]
    fn scroll_keeps_previous_offset() {
        // Kept while the cursor row is visible.
        assert_eq!(scroll_offset(5, 30, 10, 10), 5);
        // Follows the cursor out of view, up or down.
        assert_eq!(scroll_offset(5, 30, 3, 10), 3);
        assert_eq!(scroll_offset(5, 30, 20, 10), 11);
        // No blank rows at the bottom after the text shrinks.
        assert_eq!(scroll_offset(25, 28, 27, 10), 18);
        assert_eq!(scroll_offset(4, 3, 2, 10), 0);
    }

    #[test]
    fn layout_records_preedit_start() {
        let p = Preedit {
            text: "nihao".into(),
            cursor: 5,
            ..Default::default()
        };
        let l = layout_cells("ab\ncd", 4, None, Some(&p), &Theme::MOCHA, 20);
        assert_eq!(l.preedit_start, Some((1, 1)));
        assert_eq!(l.cursor, (1, 6));
        // A preedit wrapping as a whole starts on the next row.
        let l = layout_cells("abcdefgh", 8, None, Some(&p), &Theme::MOCHA, 8);
        assert_eq!(l.preedit_start, Some((1, 0)));
        let l = layout_cells("ab", 2, None, None, &Theme::MOCHA, 20);
        assert_eq!(l.preedit_start, None);
    }

    #[test]
    fn popup_placement() {
        let bounds = Rect::new(2, 1, 40, 20);
        // Below the anchor row, one column left of it.
        assert_eq!(
            popup_rect(Position::new(10, 5), (20, 3), bounds),
            Some(Rect::new(9, 6, 20, 3))
        );
        // Never left of the bounds.
        assert_eq!(
            popup_rect(Position::new(2, 5), (20, 3), bounds),
            Some(Rect::new(2, 6, 20, 3))
        );
        // Moves left to fit, and is no wider than the bounds.
        assert_eq!(
            popup_rect(Position::new(35, 5), (20, 3), bounds),
            Some(Rect::new(22, 6, 20, 3))
        );
        assert_eq!(
            popup_rect(Position::new(35, 5), (50, 3), bounds),
            Some(Rect::new(2, 6, 40, 3))
        );
        // Above the anchor row when only that side has room.
        assert_eq!(
            popup_rect(Position::new(10, 19), (20, 3), bounds),
            Some(Rect::new(9, 16, 20, 3))
        );
        // Otherwise the larger side, shrunk.
        assert_eq!(
            popup_rect(Position::new(10, 12), (20, 12), bounds),
            Some(Rect::new(9, 1, 20, 11))
        );
        assert_eq!(
            popup_rect(Position::new(10, 8), (20, 12), bounds),
            Some(Rect::new(9, 9, 20, 12))
        );
        // Nothing when neither side has 3 rows.
        let short = Rect::new(0, 0, 40, 5);
        assert_eq!(popup_rect(Position::new(0, 2), (20, 3), short), None);
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate("你好世界", 8), "你好世界");
        assert_eq!(truncate("你好世界", 7), "你好世…");
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate_start("/a/b/file", 9), "/a/b/file");
        assert_eq!(truncate_start("/a/b/file", 6), "…/file");
        assert_eq!(truncate_start("/目录/文件", 5), "…文件");
    }
}
