# 编辑文件模式候选框跟随光标：实现进度

设计见 [`plan.md`](plan.md)。

## 状态

全部完成。

- `layout_cells` 额外返回 `preedit_start`（preedit 起点的行、列），折行时取
  新行的第 0 列。
- `draw_fullscreen` 在编辑模式下让文本区占满输入框，由调用处算出锚点，交给
  `draw_candidate_popup`。
- `popup_rect` 是纯函数，负责下方 / 上方 / 截短 / 不画四种情况。
- `candidate_spans` 从 `draw_candidates` 抽出，和 `candidates_size` 共用候选
  宽度的计算；页码指示宽度抽成 `page_indicator_width`。

## 验证

- `cargo test`、`cargo clippy --all-targets` 通过，新增
  `layout_records_preedit_start`、`popup_placement` 两个单元测试。
- `scripts/e2e.sh` 新增 “edit FILE: candidates float next to the cursor”，
  检查候选在光标下方、文件末尾时翻到光标上方，以及保存结果。通过。
- 手动在 tmux 里看过横排、竖排、窄窗口（40 列，框左移并截掉放不下的候选）。
- e2e 的 “shell integration: fish” 一步等不到 `fish> ` 提示符，未改动的 main
  上同样失败，与本改动无关，没有处理。
