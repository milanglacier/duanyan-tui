# 撤销与重做：实现进度

设计见 [`plan.md`](plan.md)。

## 状态

全部完成。用户试用后合并。

- `buffer.rs`：所有修改经过 `edit`，记录为 `Change` 放进 `UndoLog`。光标移动经
  过 `move_to`（`prev_line` / `next_line` 直接调用 `seal`），结束当前这一步。
  `set_text` / `take` 连同撤销历史一起重置；`clear` 改为可撤销。
- 直接输入的字符走新的 `type_char`，其余插入（rime 上屏、粘贴、取回历史、换行）
  仍走 `insert_str`，各自一步。
- `app.rs`：新增 `undo` / `redo` 动作，组字时忽略。
- `keys.rs`：`legacy_reachable` 不再把 ctrl+? 算作可达。

## 验证

- `cargo test`、`cargo clippy --all-targets` 通过。
- `scripts/e2e.sh` 全部通过，新增 “fullscreen: undo and redo”（`C-z`、`C-_`、
  `C-y`、`M-_`）和 “--stdout: undo and redo inline”。
- KKP 下 ctrl+/ 与 ctrl+_ 分别触发撤销只有单元测试覆盖，需要在 kitty 里手动确认。
