# KKP 下 compat 的 Shift 敲击失效：实现进度

设计见 [`plan.md`](plan.md)。

## 状态

全部完成。

- `keys::to_rime`：裸修饰键只翻译 `LeftShift` / `RightShift`，其它返回 `None`。
- `app.rs` 的假引擎按 `ascii_composer` 的规则只认第一个按下的修饰键；新增
  `kkp_compat_shift_tap_with_ctrl_held`，改动前失败、改动后通过。
- `keys.rs` 的 `rime_translation` 断言 `LeftControl` 不翻译。
- `scripts/e2e.sh` 新增 KKP 一步：强制 `kitty_keyboard = "on"`，用
  `send-keys -l` 直接写入 KKP 终端会发的 CSI u 序列，覆盖“先报 Ctrl 再按
  `ctrl+l`”和单独敲 Shift。
- AGENTS.md「Terminal pitfalls」记录冲突；e2e 一节改写 KKP 的测试方法。README 在
  Shift 一段后加折叠说明。

## 与计划的偏差

- 计划以为 tmux 测不了这个场景。实际上写入原始 CSI u 序列即可：改动前的二进制
  `ctrl+l` 不切换，改动后切换。
- README 折叠段的标题改为更泛的「终端与 rime 的按键处理差异」。
- AGENTS.md 那条改为只描述 KKP 下组合键拆成多个事件与 `ascii_composer` 的冲突，
  不提我们转发什么。

## 已验证

- `cargo test`、`cargo clippy --all-targets`、`scripts/e2e.sh` 通过（e2e 结束时
  `stderr.log` 为空）。连续跑了四次，其中一次 bash 集成的三项超时，与本改动无关，
  其余三次全过。
- 用户在 kitty 里手动验证通过。
