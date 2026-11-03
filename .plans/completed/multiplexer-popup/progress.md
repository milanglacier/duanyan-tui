# multiplexer 弹窗输入：实现进度

设计见 [`plan.md`](plan.md)。

## 状态

已完成。手动验证中发现的 tmux 深浅色问题已修复（见「与计划的偏差」）。

- `crates/duanyan/src/shell/popup.sh`：POSIX sh，`case "$mux"` 分 tmux、zellij、herdr
  三支，共用 `place` 函数；另有不公开的 `place` 入口供测试使用。
- `duanyan popup <tmux|zellij|herdr> [参数…]`：与 `init` 一样在读取配置之前处理，
  以 `sh -c <脚本> duanyan <current_exe> <mux> [参数…]` 执行，退出码原样返回。
- devShell 加入 `zellij`、`herdr`、`jq`。
- README 新增「tmux / zellij / herdr 弹窗」一节和命令表的一行；AGENTS.md 的 e2e
  小节加了两条：嵌套 herdr 的注意事项，以及嵌套 tmux 下 Esc 的延迟。

## 与计划的偏差

- zellij：挪动浮窗后，脚本先用 `stty size` 等待 pty 变成新尺寸（最多约 1 秒），
  再启动 duanyan。zellij 是异步调整大小的，而 duanyan 只在启动时读一次尺寸。
- 出错时等待按键不只针对缺少 `jq`：凡是在弹窗里运行的分支（tmux 的弹窗内一侧、
  zellij、herdr）出错时，都打印错误并等 Enter，否则错误会随弹窗一起消失。
- 示例里 herdr 的快捷键用 `prefix+alt+i`：e2e 只验证过带 prefix 的写法。
- tmux 的弹窗不回复 OSC 11（窗格会回复，改 `-s` 样式也一样），duanyan 总是回退到
  深色。启动脚本把 `#{client_theme}` 传进弹窗，设成 `DUANYAN_BACKGROUND`；
  `theme.mode = "auto"` 且没有收到 OSC 11 回复时，duanyan 用它决定深浅色。
- tmux 3.8 的 `popup-style` 默认是灰色（`bg=themedarkgrey`），弹窗改用
  `-s bg=terminal,fg=terminal`。`bg=default` 不行：它表示继承 `popup-style`。

## 已验证

- `cargo test`（新增 `popup_placement`，通过 `sh` 调用脚本的 `place` 入口）和
  `cargo clippy --all-targets` 通过；`shellcheck -s sh popup.sh` 无警告。
- `scripts/e2e.sh` 全部通过。新增的检查：
  - **tmux**（内层 tmux 左右分屏）：弹窗在光标下一行、文字列与光标对齐；两行
    文字写回且没有被执行；Esc 不写回；状态栏在顶部时位置仍然正确；光标在底部
    时弹窗出现在上方。
  - **zellij**：浮窗被挪到光标下一行、文字列与光标对齐；两行文字写回且没有被
    执行。
  - **herdr**：两行文字写回且没有被执行，说明 `herdr pane send-text` 会原样传出
    ESC，bracketed paste 生效。
  - **深浅色**：外层窗口设成浅色背景，内层 tmux 设成 3.8 的灰色 `popup-style`；
    tmux 和 zellij 的弹窗里是 Latte 配色，除了状态栏的强调色没有别的背景色。改动前
    的脚本两项都失败。devShell 的 tmux 是 3.7c，默认 `popup-style` 不是灰色，所以
    e2e 显式设置它。
  - 结束时 `stderr.log` 为空。
- popup 部分单独连续跑了三次，均通过。嵌套的 multiplexer 用临时 `HOME`；跑完后
  用户真实 home 下的 herdr 集成文件没有被改动。

## 测试里遇到的问题

都已解决，`e2e.sh` 里有注释说明；Esc 的延迟另外写进了 AGENTS.md：

- NixOS 的 `/bin/sh` 是 bash，`PS1` 经过它 `exec` 后丢失；e2e 的 shell 包装脚本
  改为通过 `MUX_PS1` 传提示符。
- duanyan 退出时先清掉弹窗里的界面，脚本随后才写回，所以「弹窗变空」不代表文字
  已经到达；检查改为轮询写回的文字。
- 嵌套 tmux 下一个 Esc 大约 500ms 才到达程序（普通 pane 里也一样，与弹窗无关），
  设 `escape-time 0` 也没用；第二个 Esc 改为等第一个生效后再发。

## 手动验证

- KKP：不测。三个 multiplexer 都不转发单独的 Shift 按下（zellij、herdr 支持 KKP，
  但同样不发）。
- 深浅色：修复前，浅色终端里 tmux 的弹窗是灰底、深色配色。修复后在 kitty 里，
  tmux、zellij、herdr 的弹窗在浅色和深色主题下都正常。
- 嵌套在 tmux 里时，herdr 把窗格（包括弹窗）画成黑底，duanyan 却按外层的浅色用
  Latte；真实终端里没有这个问题，所以 e2e 不检查 herdr 的配色。
- 隐藏光标的 TUI 里，弹窗出现在窗格左下角，符合预期。
- zellij 浮窗从默认位置挪到光标旁时的闪烁可以接受。

## nix 包在 tmux 里打不开弹窗

- 现象：`nix build` 出来的 duanyan 在 tmux 里按键后弹窗一闪就消失；zellij、herdr 正常。
- 原因：nix 包原来用 `wrapProgram` 设置 `DUANYAN_LIBRIME_PATH`。`duanyan popup` 用
  `current_exe()`（未包装的二进制）在 `display-popup` 里再次运行自己，这个进程由
  tmux server 用全局环境启动，拿不到 wrapper 设置的变量，librime 加载失败；
  `popup.sh` 又把失败当成取消，静默退出。zellij 和 herdr 里输入界面是 wrapper
  的子进程，所以没事。e2e 用 `target/debug/duanyan`，没有 wrapper，测不出来。
- 修复：去掉 wrapper，改为编译时通过 `DUANYAN_BUILD_LIBRIME_PATH`、
  `DUANYAN_BUILD_RIME_SHARED_DIR` 写入默认路径（优先级在配置文件和环境变量之后）；
  `popup.sh` 只把退出码 1 当作取消，其他失败显示错误并等待 Enter。e2e 增加了
  tmux 弹窗里显示错误的检查。
