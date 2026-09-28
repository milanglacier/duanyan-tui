//! Rendering. `draw` returns the clickable regions it laid out.

use jiff::tz::TimeZone;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Focus, HitMap, Level};
use crate::config::{CandidateLayout, GlobalAction, HistoryAction, InputAction};
use crate::engine::{ImeEngine, Maintenance, Preedit};
use crate::keys::KeySpec;
use crate::theme::Theme;

/// Static facts shown in the UI.
pub struct UiContext {
    pub inline: bool,
    pub theme: Theme,
    pub candidate_layout: CandidateLayout,
    pub show_comment: bool,
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
    f.render_widget(Paragraph::new(Line::from(spans)), header);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            concat!("v", env!("CARGO_PKG_VERSION")),
            Style::default().fg(t.subtext),
        )))
        .right_aligned(),
        header,
    );

    // Input block, sized to its content.
    let text_width = inner.width.saturating_sub(4).max(1);
    let layout = layout_text(app, t, text_width);
    let max_text_rows = (area.height / 2).max(1);
    let text_rows = (layout.rows.len() as u16).clamp(1, max_text_rows);
    let cand_rows = candidate_rows(ctx, app.snapshot.candidates.len());
    let input_h = text_rows + cand_rows + 2;
    let body_top = header.bottom();
    let body_bottom = status.y;
    let input_h = input_h.min(body_bottom.saturating_sub(body_top));
    let input = Rect::new(inner.x, body_bottom - input_h, inner.width, input_h);
    let history = Rect::new(
        inner.x,
        body_top,
        inner.width,
        input.y.saturating_sub(body_top),
    );

    if history.height >= 3 {
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
    let text_area = Rect::new(
        content.x + 1,
        content.y,
        content.width.saturating_sub(2),
        text_rows.min(content.height),
    );
    render_text(f, app, t, &layout, text_area, focused && !app.show_help);
    let cand_area = Rect::new(
        content.x + 1,
        text_area.bottom(),
        content.width.saturating_sub(2),
        content.bottom().saturating_sub(text_area.bottom()),
    );
    draw_candidates(f, app, ctx, cand_area, hits);
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
    render_text(f, app, t, &layout, text_area, !app.show_help);
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
    cursor: (usize, u16),
}

fn layout_text<E: ImeEngine>(app: &App<E>, t: &Theme, width: u16) -> TextLayout {
    let text = app.buffer.text();
    let (before, after) = text.split_at(app.buffer.cursor());
    let normal = Style::default().fg(t.text);
    let mut cells: Vec<(&str, Style)> = Vec::new();
    for g in before.graphemes(true) {
        cells.push((g, normal));
    }
    let mut cursor_cell = cells.len();
    if let Some(p) = app.snapshot.preedit.as_ref().filter(|p| !p.text.is_empty()) {
        cursor_cell = push_preedit(&mut cells, p, t);
    }
    for g in after.graphemes(true) {
        cells.push((g, normal));
    }

    let width = width as usize;
    let mut rows: Vec<Vec<(String, Style)>> = vec![Vec::new()];
    let mut col = 0usize;
    let mut cursor = (0, 0);
    for (i, (g, style)) in cells.iter().enumerate() {
        if i == cursor_cell {
            cursor = (rows.len() - 1, col as u16);
        }
        if *g == "\n" {
            rows.push(Vec::new());
            col = 0;
            continue;
        }
        let w = g.width().max(if g.chars().all(char::is_control) {
            0
        } else {
            1
        });
        if col + w > width && col > 0 {
            rows.push(Vec::new());
            col = 0;
            if i == cursor_cell {
                cursor = (rows.len() - 1, 0);
            }
        }
        let row = rows.last_mut().unwrap();
        match row.last_mut() {
            Some((s, st)) if *st == *style => s.push_str(g),
            _ => row.push((g.to_string(), *style)),
        }
        col += w;
    }
    if cursor_cell == cells.len() {
        if col >= width {
            rows.push(Vec::new());
            col = 0;
        }
        cursor = (rows.len() - 1, col as u16);
    }
    TextLayout { rows, cursor }
}

/// Pushes preedit cells; returns the cell index of rime's caret.
fn push_preedit<'a>(cells: &mut Vec<(&'a str, Style)>, p: &'a Preedit, t: &Theme) -> usize {
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
        cells.push((g, style));
    }
    caret.unwrap_or(if p.cursor == 0 { start } else { cells.len() })
}

fn render_text<E: ImeEngine>(
    f: &mut Frame,
    app: &App<E>,
    t: &Theme,
    layout: &TextLayout,
    area: Rect,
    show_cursor: bool,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let (crow, ccol) = layout.cursor;
    let offset = crow.saturating_sub(area.height as usize - 1);
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
    let ind_w = page.width() as u16 + 2;
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

    let hl = Style::default()
        .bg(t.candidate_hl_bg)
        .fg(t.candidate_hl_fg)
        .add_modifier(Modifier::BOLD);
    let idx = Style::default().fg(t.candidate_index);
    let text = Style::default().fg(t.text);
    let comment = Style::default().fg(t.comment);

    let vertical = ctx.candidate_layout == CandidateLayout::Vertical;
    let mut x = area.x;
    let mut y = area.y;
    for (i, c) in snap.candidates.iter().enumerate() {
        let highlighted = i == snap.highlighted;
        let mut spans = vec![
            Span::styled(format!(" {} ", c.label), if highlighted { hl } else { idx }),
            Span::styled(format!("{} ", c.text), if highlighted { hl } else { text }),
        ];
        if ctx.show_comment
            && let Some(cm) = &c.comment
        {
            spans.push(Span::styled(format!("{cm} "), comment));
        }
        let w: u16 = spans.iter().map(|s| s.content.width() as u16).sum();
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
    let hints: Vec<(String, String)> = if app.stdout_mode {
        [
            hint("input", InputAction::Submit.name(), "输出"),
            hint("input", InputAction::Cancel.name(), "取消"),
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
        ("input", "submit") => "提交",
        ("input", "newline") => "换行",
        ("input", "focus_history") => "切到历史",
        ("input", "cancel") => "取消 / 清空",
        ("input", "left") => "左移",
        ("input", "right") => "右移",
        ("input", "prev_line") => "上一行",
        ("input", "next_line") => "下一行",
        ("input", "home") => "行首",
        ("input", "end") => "行尾",
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
            let mut spans = vec![Span::raw(format!("  {}", pad(&label, 16)))];
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
            Span::styled(format!("  {}", pad(k, 16)), dim),
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

    #[test]
    fn truncation() {
        assert_eq!(truncate("你好世界", 8), "你好世界");
        assert_eq!(truncate("你好世界", 7), "你好世…");
        assert_eq!(truncate("abcdef", 4), "abc…");
    }
}
