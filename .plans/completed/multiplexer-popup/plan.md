# multiplexer 弹窗输入的计划

在 tmux、zellij、herdr 里绑一个快捷键：按下后在当前 pane 的光标旁弹出一个小窗，
里面是 `duanyan --stdout` 的 inline 界面，提交后文字直接写进原 pane，弹窗关闭。

```sh
# ~/.tmux.conf
bind-key -n M-i run-shell -b 'duanyan popup tmux "#{pane_id}" "#{client_name}"'
```

定位、启动弹窗、写回都由一个 shell 脚本完成，duanyan 的界面和输入逻辑不改。
不写 multiplexer 插件（zellij 的 wasm 插件、herdr 的 plugin manifest）。

## 1. 已验证的前提

2026-11-02 在外层 tmux 里嵌套 tmux 3.7c、zellij 0.45.1、herdr 0.9.5 实测，
弹窗里运行的就是现有的 `duanyan --stdout`（rime 已部署）：

- 三者的弹窗里，inline 界面都能正常出候选、提交，`stderr.log` 为空，从打开到
  界面出现约 110ms。弹窗是新的 pty，光标在左上角，inline 界面从第一行开始画，
  弹窗高度等于 inline 高度时正好占满。
- **tmux**：`display-popup -x/-y` 接受数字或格式表达式，`-y` 是弹窗的**底边**；
  超出屏幕时 tmux 自动把弹窗挪回屏幕内。`#{cursor_x}`、`#{cursor_y}` 是 pane 内
  坐标，`#{pane_left}`、`#{pane_top}` 是 pane 在 window 内的坐标。
  `paste-buffer -p -t <pane>` 能写回原 pane。
- **zellij**：在浮窗里运行 `zellij action list-panes -a -j`，结果里有
  `pane_content_x/y` 和 `cursor_coordinates_in_pane`。原 tiled pane 和新浮窗都标为
  `is_focused: true`。`write-chars --pane-id terminal_N` 能写回原 pane。快捷键的
  `Run` 动作支持 `floating`、`x`、`y`、`width`、`height`、`borderless`、
  `close_on_exit`，`change-floating-pane-coordinates --pane-id` 可以移动已开的浮窗。
- **herdr**：`[[keys.command]] type = "popup"` 只能设宽高，弹窗总是居中，API 也
  不提供光标位置。原 pane 的 ID 在环境变量 `HERDR_ACTIVE_PANE_ID` 里，可以用
  `herdr pane send-text` 写回。

## 2. 打包

脚本放在 `crates/duanyan/src/shell/popup.sh`，用 `include_str!` 编进二进制，与
shell 集成的脚本相同。`duanyan popup <tmux|zellij|herdr> [参数…]` 只做一件事：
以 `sh -c <脚本> duanyan <duanyan 的路径> <mux> [参数…]` 执行它。

选这种方式的原因：

- cargo、nix、release 包都能用，不需要知道脚本装在哪里；
- 脚本和二进制版本始终一致，升级不会出现旧脚本配新二进制；
- 用户配置只有一行命令，不用先把脚本存成文件；
- 逻辑仍然是 shell，读起来和改起来都直接。

考虑过但没选的方式：

| 方式 | 问题 |
| --- | --- |
| `duanyan init <mux>` 打印脚本，用户存成文件再引用 | 多一步；升级后要记得重新生成 |
| 把脚本作为文件放进 release 包和 nix 的 `share/` | `cargo install` 装不到；各种安装方式的路径不同 |
| TPM 插件（仓库根目录放 `duanyan.tmux`） | 只适用于 tmux；以后可以在本方案之上再加 |

脚本里调用 duanyan 时用 Rust 传进来的路径（`std::env::current_exe()`），不依赖
`PATH`。

## 3. 用法

```sh
# ~/.tmux.conf
bind-key -n M-i run-shell -b 'duanyan popup tmux "#{pane_id}" "#{client_name}"'
```

```kdl
// zellij config.kdl
shared_except "locked" {
    bind "Alt i" {
        Run "duanyan" "popup" "zellij" {
            floating true; close_on_exit true; borderless true
            width 60; height 5
        }
    }
}
```

```toml
# herdr config.toml
[[keys.command]]
key = "alt+i"
type = "popup"
command = "duanyan popup herdr"
width = 62    # 含边框
height = 7
```

- tmux 的 `run-shell` 后台 job 没有「当前 pane」也没有 client，所以 pane 和 client
  由绑定里的格式变量传入。
- zellij 需要 `jq` 解析 `list-panes` 的 JSON。找不到 `jq` 时，脚本在浮窗里打印
  错误并等待按键，否则 `close_on_exit` 会让浮窗一闪而过，用户看不到原因。
- 宽高用环境变量 `DUANYAN_POPUP_WIDTH`（默认 60）、`DUANYAN_POPUP_HEIGHT`（默认 5，
  正好是横排候选的 inline 高度）调整，与 `DUANYAN_TRIGGER` 的风格一致。竖排候选
  要自己设高度：`INLINE_TEXT_ROWS (3) + page_size + 1`。herdr 的尺寸由 herdr 配置
  决定，这两个变量不起作用。

## 4. 行为

### 4.1 位置（tmux、zellij）

以光标在屏幕上的位置 `(cx, cy)` 为准：

1. 横向：弹窗左边取 `cx - 1`。inline 界面左侧留了 1 列，这样输入的文字正好和光标
   对齐。弹窗超出右边时左移，但不越过屏幕左边。
2. 纵向：光标下方放得下就放在下方（顶边 = `cy + 1`），放不下就放到上方
   （底边 = `cy - 1`）。上下都放不下时贴着屏幕底部。
3. 拿不到光标、光标隐藏（tmux `#{cursor_flag}` 为 0，zellij 的
   `cursor_coordinates_in_pane` 为 null），或者光标在 pane 外时：放在 pane 的左下角，
   底边与 pane 底边对齐。Claude Code、codex 这类自己画光标的 TUI 会走到这条路径。

脚本里写成一个函数 `place`，tmux 和 zellij 共用。

### 4.2 各 multiplexer 的流程

- **tmux**（脚本在 `run-shell` 里运行）：
  1. 用一次 `tmux display -p -c <client> -t <pane>` 取出 pane 位置和大小、光标、
     光标是否可见、window 和 client 的大小、`status`、`status-position`。状态栏
     在顶部时，pane 坐标要加上状态栏的行数。
  2. 调 `place`，再执行 `tmux display-popup -c <client> -t <pane> -B -x … -y … -w … -h … -E <命令>`，
     其中 `-y` 传底边。
  3. 弹窗里的命令：`duanyan --stdout` 写到临时文件，退出码为 0 时
     `tmux load-buffer -b duanyan-$$ <文件>`，再
     `tmux paste-buffer -p -d -b duanyan-$$ -t <pane>`。
- **zellij**（脚本在快捷键打开的浮窗里运行）：
  1. 从 `list-panes -a -j` 里找出自己（`$ZELLIJ_PANE_ID`）所在的 tab，以及这个 tab 里
     `is_focused` 为真、不是浮窗的 pane，它就是原 pane。浮窗能用的区域取这个 tab
     里所有 tiled pane 外框的并集。
  2. 调 `place`，用 `change-floating-pane-coordinates --borderless true` 把自己挪
     过去，**然后**才启动 duanyan，所以 duanyan 一启动就是最终尺寸。
  3. `duanyan --stdout` 写到临时文件，成功时用
     `zellij action write-chars --pane-id terminal_N` 写回。
- **herdr**（脚本在 herdr popup 里运行）：`duanyan --stdout` 写到临时文件，成功时
  `herdr pane send-text "$HERDR_ACTIVE_PANE_ID" …`。

用临时文件而不是 `$(…)` 接收输出：`$(…)` 会吃掉结尾的换行，而文件能保证原样
写回。

### 4.3 写回

一律以「粘贴」的方式写入，所以文字里的换行不会被当成回车：

- **tmux**：`paste-buffer -p` 只在目标程序开启了 bracketed paste 时才包上标记，
  否则按普通输入写入，也就不会出现乱码。
- **zellij、herdr**：拿不到目标程序是否开启了 bracketed paste，因此总是写入
  `ESC[200~ 文字 ESC[201~`。写入前用 `tr -d '\033'` 去掉文字里的 ESC，防止文字
  提前结束粘贴，或者夹带控制序列。不支持 bracketed paste 的程序会看到 `^[[200~`，
  这是已知的代价。
- Esc 取消（`duanyan --stdout` 以非零退出）时什么都不写。

## 5. 实现

- `crates/duanyan/src/shell/popup.sh`：POSIX sh，`case "$mux"` 分三支，加上共用的
  `place` 函数。另有一个不公开的 `place` 入口：传入屏幕、pane、光标和尺寸，打印
  结果，供测试使用。
- `main.rs`：新增子命令

  ```rust
  /// Open a popup in tmux, zellij or herdr and type into the pane under it.
  Popup {
      #[arg(value_enum)]
      mux: Mux,
      /// tmux: the pane (`#{pane_id}`) and client (`#{client_name}`).
      #[arg(trailing_var_arg = true)]
      args: Vec<String>,
  },
  ```

  与 `Init` 一样在 `Setup::resolve` 之前处理：用 `sh -c` 执行脚本，传入
  `current_exe()`、mux 名称和 `args`，退出码原样返回。
- `flake.nix`：devShell 加入 `zellij`、`herdr`、`jq`，供 e2e 使用。

## 6. 测试

单元测试（`cargo test`，通过 `sh` 调脚本的 `place` 入口）：光标下方放得下、要
放到上方、上下都放不下；靠右边时左移；pane 比弹窗窄；光标隐藏和光标在 pane 外时
退回到 pane 左下角；屏幕不从第 0 行开始（zellij 的 tab bar、tmux 状态栏在顶部）。

`scripts/e2e.sh` 新增，都用「外层 tmux 当终端」的嵌套方式，截外层屏幕：

- **tmux**：内层 tmux 左右分屏，右侧 pane 里输入 `printf abc`，从外部触发绑定的
  快捷键：弹窗出现在光标下一行、文字列与光标对齐；输入 `nihao ` 后 Enter，右侧
  pane 变成 `printf abc你好`。光标靠近底部时弹窗出现在上方。Esc 不写回。在 shell
  里提交两行文字，命令行里出现两行且没有被执行。
- **zellij**：同样的流程，检查浮窗被挪到光标旁、写回正确、两行文字没有被执行。
- **herdr**：弹窗居中，写回正确。
- 结束时 `stderr.log` 为空。

e2e 的 duanyan 配置靠 `XDG_CONFIG_HOME` 等环境变量指定。tmux 弹窗继承的是 tmux
server 的环境，所以要用 `set-environment -g` 把这些变量设到内层 tmux server 上。

手动：在 kitty 里分别跑三种 multiplexer，确认 Shift 切换中英文（KKP）和深浅色
探测正常，以及 zellij 浮窗从默认位置挪过去时的闪烁可以接受。

## 7. 文档

- README 新增「在 tmux / zellij / herdr 里弹窗输入」一节，给出三段配置，说明
  `DUANYAN_POPUP_WIDTH` / `DUANYAN_POPUP_HEIGHT`、zellij 需要 `jq`、herdr 的弹窗只能
  居中，以及「不支持 bracketed paste 的程序会看到乱码」的限制。
- 命令表加入 `duanyan popup <tmux|zellij|herdr>`。

## 8. 要写进 AGENTS.md 的坑

- 在 herdr 里跑嵌套 herdr：要先 `env -u` 掉所有 `HERDR_*` 变量（否则 herdr 拒绝
  嵌套，`server stop` 之类的命令还会打到用户正在用的 server）。临时配置里要设
  `onboarding = false`，否则引导之后会弹出 integrations 设置页，发过去的 Enter 会
  在用户的真实 home 下安装或更新 agent 集成。
- tmux 的 `display-popup -y` 是底边，不是顶边。
- zellij 浮窗里看到的 `is_focused` 有两个为真。

## 9. 不做

- 弹窗用主题背景色铺满、竖排候选自动算高度：都要改 duanyan 本身，首版不做。
- 打印脚本供用户修改（如 `duanyan init popup`）：等有人需要再加。
- `--config` 传给弹窗里的 duanyan：弹窗里总是用默认配置路径。
- zellij wasm 插件、herdr plugin manifest、herdr 弹窗跟随光标。
- 从 zellij 浮窗里触发时写回那个浮窗。
- 写回前检测目标程序是否开启了 bracketed paste（zellij、herdr 不提供这个信息）。
