# tmux 3.8 浮动 pane：实现进度

设计见 [`plan.md`](plan.md)。

## 状态

已实现，待在 kitty 里手动验证。

- `flake.lock`：nixpkgs 更新到 2026-11-03，devShell 的 tmux 为 3.8。
- `popup.sh`：现有的 `tmux display` 里加 `x#{pane_floating_flag}` 检测浮动 pane；
  支持时用 `new-pane -O -K -A -B none` 打开，坐标用 window 坐标；pane 里先关掉
  `remain-on-exit`。`DUANYAN_POPUP_TMUX=popup` 强制走旧路径，只给 e2e 用。
- `scripts/e2e.sh`：tmux 部分改成 `tmux_popup MODE`，`float` 和 `popup` 各跑一遍。
- README 的深浅色说明。

## 与计划的偏差

- rime-data 更新后，`luna_pinyin` 给 `ascii_mode` 加了 `abbrev: [中, Ａ]`，状态栏
  的西文模式显示「Ａ」。基线 e2e 因此有两项失败（与浮动 pane 无关），检查改为
  「Ａ」。

## 已验证

- `cargo test`、`cargo clippy --all-targets` 通过；`shellcheck -s sh popup.sh`
  无警告。
- `scripts/e2e.sh` 全部通过（120 项）。浮动 pane 部分：modal 浮动 pane、位置
  （含状态栏在顶部、光标在底部）、OSC 11 浅色、两行写回且未执行、Esc 不写回、
  zoom 保持、`remain-on-exit on` 时不留 dead pane、错误等待 Enter。popup 部分
  跑原有检查。
- 变异测试：去掉 `remain-on-exit off` 后「no pane left behind」失败；去掉 `-A`
  后检查仍然通过，原因见计划第 2 节。

## 手动验证

- [ ] kitty + tmux 3.8：浅色、深色主题下浮动 pane 的配色；光标旁的位置；zoom。
