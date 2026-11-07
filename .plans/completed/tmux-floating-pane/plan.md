# tmux 3.8 浮动 pane 输入框的计划

tmux 3.8 新增了浮动 pane（`new-pane`）。`duanyan popup tmux` 在支持浮动 pane 的
tmux 上改用浮动 pane，旧版本仍用 `display-popup`。用户的绑定不用改：

```sh
bind-key -n M-i run-shell -b 'duanyan popup tmux "#{pane_id}" "#{client_name}"'
```

## 1. 为什么

浮动 pane 是普通 pane，`display-popup` 的两个绕路都不需要了：

- popup 不回复 OSC 11，所以启动脚本把 `#{client_theme}` 当作
  `DUANYAN_BACKGROUND` 传进去；浮动 pane 自己会回复 OSC 11。
- tmux 3.8 的 `popup-style` 默认是灰色，所以 popup 用
  `-s bg=terminal,fg=terminal`；浮动 pane 用的是 pane 的 `window-style`。

旧版本 tmux 仍然走 popup，这两段代码保留，只在 popup 分支里用。

## 2. 已验证的前提

2026-11-07 在外层 tmux 里嵌套 tmux 3.8 实测：

- 浮动 pane 回复 OSC 11（外层窗口的浅色背景 `rgb:efef/f1f1/f5f5`），同样的探测
  在 `display-popup` 里只收到 DA1。
- `new-pane -X/-Y` 是 **window** 坐标，状态栏在顶部时不需要再加状态栏的行数，
  这和 popup 的 client 坐标不同。`-B none` 时没有边框，内容区就从 `-X/-Y` 开始；
  `pane-border-status top` 也不会给它加标题行。
- 尺寸在创建时就是最终值，不像 zellij 那样要等异步 resize。
- 浮动 pane 退出后，焦点自动回到原 pane。
- window 已 zoom 时，不加 `-A` 的 `new-pane` 会取消 zoom，并且退出后不恢复；
  加 `-A` 时 window 保持 zoom，浮动 pane 显示在上面。modal（`-O`）的 pane 不加
  `-A` 也保持 zoom，但手册没写，所以仍然加 `-A`；e2e 因此分辨不出有没有 `-A`。
- 用户开了 `remain-on-exit` 时，浮动 pane 退出后会留下一个 dead pane。在 pane
  里先执行 `tmux set -p -t "$TMUX_PANE" remain-on-exit off` 就不会。
- `-O -K`（modal，所有按键包括 prefix 都交给 pane）下，外部发来的 `C-b c` 原样
  到达 pane 里的程序，没有新建 window。
- 格式变量 `#{pane_floating_flag}` 在 3.8 展开为 `0`/`1`，在旧版本里是未知变量，
  展开为空。

## 3. 行为

- **检测**：现有的那次 `tmux display -p` 里加一项 `x#{pane_floating_flag}`，结果
  为 `x` 就是不支持浮动 pane 的旧版本。不看 `#{version}`：版本号有 `3.8a`、
  `next-3.9`、`master` 等写法，不如直接测能力。
- **打开**：

  ```sh
  tmux new-pane -O -K -A -B none -t <pane> -X x -Y y -x w -y h <命令>
  ```

  - `-O -K`：modal，和 popup 一样所有按键都交给 duanyan，也不能点到别的 pane。
    不加 `-C`、`-D`：点外面或按 Esc 直接关闭会丢掉正在输入的文字，而 Esc 本来
    就是 duanyan 自己处理的。
  - `-A`：zoom 的 window 保持 zoom。
  - `-B none`：没有边框，与 popup 的 `-B` 一致。
- **位置**：共用 `place`。屏幕区域是 window 可见的部分，即 `(0, 0)` 起、
  `min(window, client 去掉状态栏)` 大小；光标和 pane 用 window 坐标，不加状态栏
  偏移。
- **pane 里**：`popup tmux --inside <pane> float`。先关掉本 pane 的
  `remain-on-exit`，其余和 popup 相同：`duanyan --stdout` 写到临时文件，成功时
  `load-buffer` + `paste-buffer -p` 写回原 pane，出错时等 Enter。
- **测试开关**：环境变量 `DUANYAN_POPUP_TMUX=popup` 强制走 `display-popup`，只给
  e2e 用（devShell 的 tmux 是 3.8，否则测不到旧路径），不写进 README。

## 4. 实现

- `crates/duanyan/src/shell/popup.sh`：tmux 分支按上面的检测分成 popup 和浮动
  pane 两条路径；`--inside` 的第三个参数是 `float` 或 popup 的深浅色。
- `flake.lock`：更新 nixpkgs，devShell 的 tmux 变成 3.8。rime-data 同时更新到
  `0-unstable-2026-10-08`，它给 `ascii_mode` 加了 `abbrev: [中, Ａ]`，状态栏的
  西文模式从「西」变成「Ａ」，e2e 的两处检查跟着改。

## 5. 测试

`scripts/e2e.sh` 的 tmux 部分改成函数，分别以浮动 pane 和
`DUANYAN_POPUP_TMUX=popup` 各跑一遍现有的检查（位置、浅色、写回两行、Esc、状态栏
在顶部、光标在底部、错误等待 Enter）。浮动 pane 另外检查：

- 打开时有一个 modal 的浮动 pane；
- 浅色来自 OSC 11：不再传 `DUANYAN_BACKGROUND`，浅色检查仍然通过；
- zoom 的 window 里打开、取消后仍然是 zoom，位置在光标旁；
- `remain-on-exit on` 时取消后没有留下 dead pane。

手动：在真实终端里用 tmux 3.8 确认深浅色、位置正常。

## 6. 文档

- README 的注意事项：tmux 3.8 及以上用浮动 pane，深浅色直接跟随终端；旧版本用
  popup，跟随 tmux 识别到的深浅色。
- AGENTS.md 不改：坐标、`-A` 和 `DUANYAN_POPUP_TMUX=popup` 在 `popup.sh` 和
  `e2e.sh` 的注释里都有说明。

## 7. 不做

- 不提供面向用户的开关来选择 popup 或浮动 pane。
- 不去掉 popup 路径：tmux 3.8 以前的版本还需要它。
- 绑定里直接用 `new-pane`（它的 `-X/-Y` 接受格式表达式）而不经过 `run-shell`：
  写回仍然需要脚本，绑定也就不必改。
