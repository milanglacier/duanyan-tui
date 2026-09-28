# 端砚 Duanyan

基于 [librime](https://github.com/rime/librime) 的终端中文输入草稿板。在没有系统输入法的环境里（SSH、TTY、未配置 fcitx 的 Wayland 会话等）用 rime 打字，提交后复制到剪贴板，或者作为选择器把文字输出到 stdout。

- 全屏模式：输入框 + 历史面板。Enter 提交：记入历史，并通过 OSC 52 复制到剪贴板。
- `--stdout` 模式：在光标下方占几行的 inline 界面，提交一次即把文字原样写到 stdout 并退出。例如：

  ```sh
  git commit -m "$(duanyan --stdout)"
  ```

## 安装

### Nix

```sh
nix run github:milanglacier/duanyan-tui
nix profile install github:milanglacier/duanyan-tui
```

在 NixOS 或 home-manager 配置里，把本仓库加为 flake input，然后使用
`inputs.duanyan.packages.${system}.default`。

`packages.default` 的 wrapper 通过 `DUANYAN_LIBRIME_PATH` 注入 nix 的 librime。想同时打包方案数据时：

```nix
duanyan.override { rimeDataPackages = [ pkgs.rime-data ]; }
```

### 其它 Linux / macOS

需要 librime ≥ 1.8。端砚在运行时 `dlopen` librime，不在编译期链接：

```sh
cargo install --git https://github.com/milanglacier/duanyan-tui duanyan
```

librime 按以下顺序查找：
1. 配置项 `rime.librime_path`
2. 环境变量 `$DUANYAN_LIBRIME_PATH`
3. 系统加载器（`librime.so.1` / `librime.1.dylib`）
4. 常见路径，包括 NixOS 的 profile 目录和 macOS Squirrel 自带的 librime

运行 `duanyan info` 查看实际解析到的库和目录。

## 目录

| 用途 | 默认位置 |
| --- | --- |
| 配置文件 | `~/.config/duanyan/config.toml` |
| rime 用户目录 | `~/.config/duanyan/rime` |
| rime 共享目录 | 自动探测：`$DUANYAN_RIME_SHARED_DIR`、`$XDG_DATA_DIRS/*/rime-data`、`/usr/share/rime-data`、Squirrel 的 SharedSupport |
| 历史、日志、实例锁 | `~/.local/state/duanyan/` |

共享目录对 rime 是只读的，可以与 fcitx5 等其它前端共用。用户目录里有 rime 写入的 `build/` 和用户词典，必须独立。找不到共享目录时，用户目录同时充当共享目录；这和 fcitx5-rime、Squirrel 的常见用法一致，把整套方案放进 `~/.config/duanyan/rime` 即可。

`duanyan --print-default-config` 输出带注释的完整默认配置。

## 部署

- 首次运行时自动部署。
- 之后默认（`rime.deploy_on_startup = "notify"`）只在启动时检查用户目录和共享目录里的顶层 `*.yaml`。发现改动时，状态栏提示 `⚠ 配置已变更 · F5 部署`，并继续使用旧的部署结果。
- `auto` 表示检测到改动就自动部署，`never` 表示不检查。
- 改动 `lua/`、`opencc/` 等子目录不会被检测到，需要按 F5（或运行 `duanyan deploy`）手动部署。
- 命令行：`duanyan deploy [--full]`、`duanyan sync`。

## 按键

所有按键先交给 rime。rime 没有处理的按键，才由端砚处理，例如不在组字时的 Enter、Tab、Ctrl+A。rime 方案自带的键位（翻页、方案选单、`ascii_composer` 等）因此照常生效。

处理顺序：`keybinding.global`（不经过 rime）→ `keybinding.compat`（转换后发给 rime）→ rime → 当前焦点的键位表。

终端无法单独报告 Shift、Ctrl 等修饰键的按下与松开，所以 rime 常用的「单按 Shift 切换中英」在多数终端里无法直接使用。有两种途径：

- **compat 映射（始终生效）**：默认 `alt+l` → `Shift_L`、`alt+r` → `Shift_R`，发送一次「按下 + 松开」。
- **kitty keyboard protocol**：kitty、foot、WezTerm、Ghostty 等终端支持时（`tui.kitty_keyboard = "auto"` 会自动探测），单独的 Shift 键可以原生使用。tmux 等复用器下探测可能不准，可以设为 `"off"`。

键位写法为 `[ctrl+][alt+][super+][shift+]<key>`：

- 可打印键写成终端实际产生的字符，区分大小写：`alt+K` 与 `alt+k` 不同，`alt+:` 与 `alt+;` 不同。
- `+` 键本身写作 `plus`。
- 命名键：`enter tab backtab esc backspace delete insert home end pageup pagedown up down left right space f1..f24`。

没有 kitty keyboard protocol 时：

- 这几组组合与右边的键无法区分：`ctrl+[` ≡ `esc`、`ctrl+i` ≡ `tab`、`ctrl+m` ≡ `enter`。
- `ctrl+,`、`ctrl+K`、`shift+enter` 等组合发不出来。F1 帮助页会把这类绑定标为「当前终端不可用」。

完整键位见默认配置和 F1 帮助页。常用键：

| 键 | 作用 |
| --- | --- |
| Enter | 提交（rime 未组字时） |
| Ctrl+J | 换行 |
| Ctrl+P / Ctrl+N | 上一行 / 下一行（组字时交给 rime 移动候选） |
| Tab | 切到历史面板（未组字时） |
| 历史面板：j/k、y、Enter、d | 移动、复制、取回编辑、删除 |
| F1 / F5 / F6 / Ctrl+C | 帮助 / 部署 / 同步 / 退出 |

## 剪贴板

默认使用 OSC 52，SSH 下同样可用。在 tmux 里需要 `set -g set-clipboard on`。

也可以改用外部命令：

```toml
[clipboard]
backend = "command"
command = ["wl-copy"]   # 不填则自动探测 wl-copy / xclip / xsel / pbcopy
```

## 多实例

同一时间只有一个端砚能打开用户词典。已有端砚在运行时，再打开的端砚照常出候选，但不学习新词，也不能部署或同步；状态栏会提示「已有端砚在运行，不学习新词」。

## 开发

```sh
nix develop        # rust 工具链、librime、rime-data，并导出测试所需的环境变量
cargo test         # 含真实 librime 的集成测试和 FFI 布局测试
scripts/e2e.sh     # 在 tmux 里运行真实二进制的端到端测试
cargo run -p rime-dl --example repl -- /tmp/rime-user   # 行式 REPL，手动调试 rime
```

## 许可证

[GPL-3.0-or-later](LICENSE)
