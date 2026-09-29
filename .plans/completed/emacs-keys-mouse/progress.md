# Emacs 按键与输入框鼠标支持：实现进度

设计见 [`plan.md`](plan.md)。

## 状态

全部完成，`cargo test`、`cargo clippy --all-targets`、`scripts/e2e.sh` 通过。

- `Buffer`：`word_start_before` 从 `kill_word` 抽出，与 `word_left` 共用；
  `word_right`、`buffer_start`、`buffer_end`、`set_cursor`、`delete_range`。
  换行前后的词边界按整个 CRLF 处理，光标不会落在 CR 与 LF 之间。
- `InputAction` 新增 `word_left` / `word_right` / `buffer_start` / `buffer_end`，
  默认 `alt+b` / `alt+f` / `alt+<` / `alt+>`；compat 默认改为 `ctrl+l` / `ctrl+r`。
- `ui.rs`：`layout_cells` 为每行产出 `RowHit`（格子的列、宽度、前后偏移，行尾偏移），
  并给选区内的格子加 `selection_bg`；`scroll_offset` 沿用上一帧的
  `HitMap::text_scroll`。全屏模式把可见行写入 `HitMap::text`。
- `App`：`selection` + `drag_anchor`；按下 / 拖动 / 松开见 `App::mouse`。点击时
  光标一律落在被点字符左边；拖选包含起点和终点下的字符。

## 验证

- e2e：`ctrl+l` / `ctrl+r` 切换中英文；`alt+b` / `alt+f` / `alt+<` / `alt+>` 移动；
  用注入的 SGR 序列测点击定位、拖动复制（`tmux show-buffer`）、Backspace 删除选区。
- 手动：tmux 里拖选汉字时选区高亮正确（`capture-pane -e` 看到 `selection_bg`）。
