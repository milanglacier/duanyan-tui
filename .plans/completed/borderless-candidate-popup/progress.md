# 编辑文件模式去掉候选框边框：实现进度

设计见 [`plan.md`](plan.md)。

## 状态

全部完成。用户试用后认为无边框也足够清楚，为了界面一致而合并。

- `draw_candidate_popup` 去掉边框，保留 `Clear` 和 `t.bg` 填充，尺寸改为
  `(w + 1, h)`。
- `popup_rect` 从锚点列开始放置，最小高度降为 1 行。

## 验证

- `cargo test`、`cargo clippy --all-targets` 通过，`popup_placement` 按新规则
  改写，新增 1 行 / 2 行 `bounds` 的用例。
- `scripts/e2e.sh` 全部通过，候选与光标行的行差由 2 改为 1。
