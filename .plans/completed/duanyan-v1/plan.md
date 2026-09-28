# 端砚 Duanyan v1 计划

基于 librime 的全屏 TUI 中文输入草稿板，Rust 实现。UI 原型曾放在
`references/design/端砚 Duanyan TUI.html`（Catppuccin Mocha / Latte 两套），
已在提交 e7671f1 中删除，需要时可从该提交的父提交取回。

## 1. 产品定位

- **剪贴板草稿板**（默认模式）：全屏 TUI，用 rime 打字 → 上屏到输入缓冲（可多行）→
  Enter 提交进历史并自动复制到剪贴板 → 清空缓冲。适用于没有系统 IME 的环境
  （SSH、TTY、未配置 fcitx 的 Wayland 会话等）。
- **stdout 选择器**（`duanyan --stdout`）：inline 紧凑界面，界面画到 `/dev/tty`，
  提交一次即把文本写到 stdout 并退出，例如 `git commit -m "$(duanyan --stdout)"`。
  Esc / Ctrl+C 取消时以非零退出码退出。提交内容仍然记入历史。

## 2. 决策记录

| 主题 | 决定 |
| --- | --- |
| TUI 框架 | ratatui + crossterm |
| librime 加载 | 运行时 dlopen（`libloading`），只 dlsym `rime_get_api` |
| 键位总方案 | rime 优先（方案一）；compat 映射**始终生效**；KKP 为可选增强 |
| KKP | `tui.kitty_keyboard = "auto" \| "on" \| "off"`，默认 auto |
| 键位配置语法 | `动作 = 键 \| [键, ...]`，按 global / compat / input / history 分表 |
| 焦点 | 输入框 / 历史面板两焦点；切换键可配置；仅在 rime 无组字时切换 |
| 输入缓冲 | 多行（Ctrl+J 插入换行），readline 式编辑；Enter（rime 未处理时）提交 |
| submit | 入历史 + 自动复制（`general.copy_on_submit`，默认 true）+ 清空 |
| 剪贴板 | 默认 OSC52，可配置改用本地命令后备 |
| 用户数据目录 | `~/.config/duanyan/rime` |
| 共享数据目录 | 配置 > 环境变量 > 扫描 `$XDG_DATA_DIRS` > 常见路径 |
| 部署 | 默认 `notify`：启动时只做 mtime 检测并提示；首次必部署；手动 deploy/sync |
| 多实例 | v1 允许多实例、后来者降级并提示；engine 抽象为消息接口，预留 daemon |
| 历史 | 持久化到 `$XDG_STATE_HOME/duanyan/history.jsonl`，可配置 |
| 主题 | 内置 Mocha / Latte，`auto` 用 OSC 11 探测，`[theme.colors]` 可逐项覆盖 |
| 状态栏 | 按当前方案 `switches` 动态生成，支持鼠标点击切换 |
| 候选栏 | 默认横排，可配竖排；显示 comment；每页数量由方案决定 |
| 粘贴 | bracketed paste 绕过 rime 直接插入缓冲（保留换行，`\r\n` / `\r` 统一为 `\n`）；组字中粘贴则先丢弃组字 |
| 输出 | 剪贴板与 stdout 都原样输出缓冲文本，不额外添加或去掉换行 |
| recall | 历史面板 Enter 把条目插入到输入缓冲光标处，切回输入焦点 |
| 平台 | Linux + macOS；不支持 Windows |
| 工具链 | nixpkgs 自带 rustc/cargo/clippy/rustfmt/rust-analyzer |
| flake 输出 | `devShells.default` + `packages.default` |
| 测试数据 | nixpkgs `rime-data`（luna_pinyin 等） |

## 3. 架构

```
┌──────────── main thread ─────────────┐        ┌──── engine thread ────┐
│ crossterm events ─► KeyRouter ─┬─────┼─cmd───►│ RimeEngine (FFI)      │
│                                │     │        │  session / deploy     │
│ AppState ◄─────── snapshot ────┴─────┼◄─evt───│  notification handler │
│ ratatui render                       │        └───────────────────────┘
└──────────────────────────────────────┘
```

- **engine 线程**持有 librime（初始化、session、部署）。主线程只通过
  `EngineCommand` / `EngineEvent` 两个枚举与它通信，二者 `derive(Serialize,
  Deserialize)`。以后改成 daemon 时，把 channel 换成 unix socket 即可，UI 侧不动。
- **快照模型**：每次 `process_key` 之后 engine 返回一个拥有所有权的 `ImeSnapshot`
  （commit 文本、preedit + 光标 + 选区、raw input、候选页、高亮、页码、is_last_page、
  schema id/name、各 switch 状态、是否 composing）。UI 不持有任何 rime 指针。
- rime 的 notification handler 是在 rime 线程里调用的 C 回调，只负责把
  `(type, value)` 推进 channel，转成 `EngineEvent::Notification`。

### 3.1 crate 布局（cargo workspace）

```
crates/
  rime-dl/            # librime 的 dlopen 绑定 + 安全封装，不含任何 UI 逻辑
    src/ffi.rs        # 手写 #[repr(C)] RimeTraits/RimeApi/RimeContext/... 镜像 rime_api.h
    src/loader.rs     # 库路径探测、dlopen、版本检查
    src/api.rs        # 安全封装：Session、Context/Commit/Status 转为 owned 结构、Config 读取
    src/keysym.rs     # X11 keysym 表、rime 键名解析（"Control+grave"、"Release+Shift_L"）
  duanyan/            # 二进制
    src/main.rs       # CLI（clap）：默认 TUI、--stdout、--fullscreen、deploy、sync、
                      #   --print-default-config
    src/config.rs     # config.toml 反序列化 + 默认值 + 校验
    src/paths.rs      # XDG 目录、librime / shared_data_dir 探测
    src/engine.rs     # EngineCommand/EngineEvent、engine 线程、ImeEngine trait
    src/keys.rs       # 终端键表示法解析（"alt+l"）、crossterm KeyEvent → rime (keycode, mask)
    src/router.rs     # 按键分发管线（global → compat → rime → 焦点表）
    src/buffer.rs     # 多行输入缓冲（光标、readline 编辑、行间移动、按显示宽度计算）
    src/history.rs    # 历史存储（jsonl）与面板状态
    src/clipboard.rs  # OSC52 / 命令后备
    src/instance.rs   # 实例锁
    src/theme.rs      # 调色板、OSC 11 探测
    src/ui/           # ratatui widgets：header、history、input、candidates、statusbar、help
```

### 3.2 FFI 要点

- 只 dlsym `rime_get_api`，拿到 `RimeApi*` 函数表。rime 的结构体靠 `data_size`
  做版本兼容，读取某个函数指针前先检查 `data_size` 是否覆盖该字段的偏移。
  较新的函数（`change_page`、`highlight_candidate` 等）不可用时降级：翻页改为发
  Page_Up/Page_Down 键。
- 最低支持版本 librime 1.8（有 `get_state_label_abbreviated`，并且 traits 带
  `log_dir`）。启动时调用 `get_version` 检查，低于该版本就报错退出。nixpkgs 当前是 1.17.0。
- `RimeTraits`：
  - `app_name = "rime.duanyan"`，`distribution_name = "Duanyan"`，
    `distribution_code_name = "duanyan"`，`distribution_version` = crate 版本。
  - `log_dir = $XDG_STATE_HOME/duanyan/log`：**必须设置并预先创建**。librime 不会
    创建该目录；目录不存在时 glog 每条日志都往 stderr 打
    `Could not create logging file`，会把 TUI 画面弄乱（M1 实测）。级别用
    `rime.log_level`（默认 warning）。
- 结构体镜像手写，不用 bindgen，可以省掉构建期对头文件的依赖。
  增加一个 `#[test]`：devShell 下用 `cc` 读取 `rime_api.h` 里的 `sizeof` /
  `offsetof`，与 Rust 侧逐字段比对（只在设置了 `RIME_INCLUDE_DIR` 时运行）。

## 4. 路径与配置

### 4.1 目录

| 用途 | 默认 | 覆盖方式 |
| --- | --- | --- |
| 配置文件 | `$XDG_CONFIG_HOME/duanyan/config.toml`（`~/.config/...`） | `--config` |
| rime user_data_dir | `$XDG_CONFIG_HOME/duanyan/rime` | `rime.user_data_dir` |
| rime shared_data_dir | 自动探测（见下） | `rime.shared_data_dir` / `DUANYAN_RIME_SHARED_DIR` |
| 历史 / 日志 / 锁 | `$XDG_STATE_HOME/duanyan/`（`~/.local/state/...`） | `history.path` |

macOS 也使用 XDG 风格路径（`~/.config`、`~/.local/state`），不使用 `~/Library`。

**shared 和 user 的区别**：`shared_data_dir` 对 rime 是只读的，可以安全复用
fcitx5 等其它前端的 rime-data。`user_data_dir` 是 rime 写入的地方（`build/`、
`*.userdb`（LevelDB，进程独占锁）、`installation.yaml`、`user.yaml`、`sync/`），
所以必须和其它前端分开。同名文件放在 user 目录里会覆盖 shared 目录里的。

### 4.2 librime 探测顺序

1. `rime.librime_path`（配置）
2. `$DUANYAN_LIBRIME_PATH`（flake 的 wrapper 用 `--set-default` 注入 nix 的
   librime；devShell 也导出这个变量）
3. 按库名 dlopen `librime.so.1`（Linux）/ `librime.1.dylib`（macOS），交给系统加载器搜索
4. 常见路径：
   - Linux：`/run/current-system/sw/lib`、`/etc/profiles/per-user/$USER/lib`、
     `~/.nix-profile/lib`、`/usr/lib`、`/usr/lib64`、`/usr/lib/x86_64-linux-gnu`、
     `/usr/lib/aarch64-linux-gnu`、`/usr/local/lib`
   - macOS：`/Library/Input Methods/Squirrel.app/Contents/Frameworks/librime.1.dylib`、
     `/opt/homebrew/lib`、`/usr/local/lib`

全部失败时报错，并列出尝试过的路径。

### 4.3 shared_data_dir 探测顺序

1. `rime.shared_data_dir`
2. `$DUANYAN_RIME_SHARED_DIR`
3. `$XDG_DATA_DIRS` 中每一项下的 `rime-data`（NixOS 下包含
   `/run/current-system/sw/share`、`/etc/profiles/per-user/$USER/share`、`~/.nix-profile/share`）
4. `/usr/share/rime-data`、`/usr/local/share/rime-data`；macOS 另加
   `/Library/Input Methods/Squirrel.app/Contents/SharedSupport`、`/opt/homebrew/share/rime-data`

全部失败时以 user_data_dir 作为 shared 目录，不报错也不警告。这是正常用法：
fcitx5-rime 的 `~/.local/share/fcitx5/rime`、Squirrel 的 `~/Library/Rime` 都是把用户配置
和整套方案放在同一个目录里。实际使用的目录在帮助页（F1）和 `--version --verbose`
里显示，便于排查。

### 4.4 config.toml 完整示例（即默认值）

```toml
[general]
copy_on_submit = true

[rime]
# librime_path = "/path/to/librime.so.1"   # 默认自动探测
# shared_data_dir = "/path/to/rime-data"    # 默认自动探测
# user_data_dir = "~/.config/duanyan/rime"
deploy_on_startup = "notify"                # notify | auto | never
log_level = "warning"                       # info | warning | error

[tui]
kitty_keyboard = "auto"                     # auto | on | off
mouse = true
candidate_layout = "horizontal"             # horizontal | vertical
show_candidate_comment = true

[theme]
mode = "auto"                               # auto | dark | light
# [theme.colors]                            # 逐项覆盖，键名见 theme.rs
# accent = "#89b4fa"

[clipboard]
backend = "osc52"                           # osc52 | command
# command = ["wl-copy"]                     # backend = "command" 时使用；不填则自动探测

[history]
persist = true
max_entries = 1000
# path = "~/.local/state/duanyan/history.jsonl"

[keybinding.global]        # 永不进 rime
quit   = "ctrl+c"
help   = "f1"
deploy = "f5"
sync   = "f6"

[keybinding.compat]        # 键名 = rime 键（rime 自己的语法）；值 = 终端键
Shift_L = "alt+l"          # 目标是单独的修饰键时，自动发送 press + release
Shift_R = "alt+r"

[keybinding.input]         # 仅在 rime 未处理时触发
submit        = "enter"
newline       = "ctrl+j"   # 在光标处插入换行
focus_history = "tab"      # 仅在 rime 没有组字时生效
cancel        = "esc"      # --stdout：取消并退出；否则清空缓冲
left          = ["left", "ctrl+b"]
right         = ["right", "ctrl+f"]
prev_line     = ["ctrl+p", "up"]    # 上一行，保持目标显示列
next_line     = ["ctrl+n", "down"]  # 下一行
home          = ["home", "ctrl+a"]  # 当前行行首
end           = ["end", "ctrl+e"]   # 当前行行尾
backspace     = ["backspace", "ctrl+h"]   # 在行首时与上一行合并
delete        = ["delete", "ctrl+d"]      # 在行尾时与下一行合并
kill_word     = "ctrl+w"
kill_to_start = "ctrl+u"   # 删到当前行行首
kill_to_end   = "ctrl+k"   # 删到当前行行尾；已在行尾时删掉换行（同 Emacs）

[keybinding.history]       # 历史焦点下，按键完全不进 rime
next        = ["j", "down"]
prev        = ["k", "up"]
copy        = "y"
recall      = "enter"
delete      = "d"
focus_input = ["tab", "esc", "i"]
```

- 用户配置与默认值**按动作合并**：用户写了 `submit = "alt+enter"` 就替换
  `submit` 的默认绑定；写成 `submit = []` 则解绑。
- 加载时做校验：未知动作名、无法解析的键、非法的 rime 键名、同一张表内同一个键
  绑定到多个动作，都在启动时报错（指明所在行），不静默忽略。

### 4.5 终端键表示法

语法：`[ctrl+][alt+][super+][shift+]<key>`，修饰键的书写顺序不限，名字大小写不敏感。

`<key>` 分两类：

1. **命名键**（小写）：`enter tab backtab esc backspace delete insert home end
   pageup pagedown up down left right space plus f1..f24`。
2. **单个可打印字符**：按**终端实际产生的字符**书写，**区分大小写**，shift 已经
   体现在字符里。
   - `alt+k` 与 `alt+K` 是两个不同的键，后者即 Alt+Shift+k。
   - `alt+;` 与 `alt+:` 不同，`ctrl+,`、`ctrl+[`、`alt+/`、`alt+.` 都直接写字符。
   - `+` 本身写作 `plus`（`ctrl+plus`）；`ctrl++` 也接受，按“最后一个 `+` 是键”解析。
   - 含 `"`、`\` 的键在 TOML 里用单引号字面量，例如 `'alt+"'`、`'ctrl+\'`。

规范化规则（配置里的键和收到的事件都先规范化，再做匹配）：

- 可打印字符不再单独带 shift：收到 `shift + 'K'` 或（KKP 下）`shift + 'k'` 且
  alternate key 为 `'K'`，都规范化为 `K`。配置里写 `shift+k` 视为 `K` 的别名；
  对非字母字符写 `shift+`（如 `shift+;`）直接报错，因为 shift 后得到的字符依赖
  键盘布局，请直接写 `:`。
- 命名键保留 shift：`shift+enter`、`shift+f1`、`shift+tab`（等同 `backtab`）。
- `ctrl+<字母>` 按小写规范化；`ctrl+K`（即 ctrl+shift+k）只有 KKP 下才能与
  `ctrl+k` 区分。

传统终端（KKP 未开启）的编码限制：

- 以下几组在传统编码下字节相同，无法区分：`ctrl+[` ≡ `esc`、`ctrl+i` ≡ `tab`、
  `ctrl+m` ≡ `enter`、`ctrl+h` 在部分终端上 ≡ `backspace`。KKP 未开启时，匹配器把
  左边的写法当作右边的别名；KKP 开启后按不同键处理。
- `ctrl+j` 在 raw mode 下是 0x0A，而回车发的是 0x0D，所以即使在传统终端里也能与
  `enter` 区分。
- 部分组合在传统编码下根本发不出来（如 `ctrl+,`、`ctrl+;`、`ctrl+1`、`ctrl+K`、
  `shift+enter`）。配置加载时对这类绑定不报错；若当前 KKP 未开启，帮助页把它们
  标记为“当前终端不可用”。
- 冲突检查按当前是否启用 KKP 进行：KKP 关闭时，`ctrl+[` 与 `esc` 绑定到同一张表的
  不同动作算冲突，启动时报错。

## 5. 按键管线

### 5.1 分发顺序（输入焦点）

```
KeyEvent
  │
  ├─ 1. keybinding.global 命中 ─────────► 执行端砚动作（不进 rime）
  ├─ 2. keybinding.compat 命中 ─────────► 转成 rime 键序列 → process_key
  ├─ 3. 翻译为 (keycode, mask) → rime process_key
  │       ├─ 返回 true  → 取 commit 插入缓冲；刷新快照
  │       └─ 返回 false ↓
  ├─ 4. keybinding.input 命中 ──────────► 执行端砚动作
  └─ 5. 可打印字符（例如 ascii_mode 下）─► 直接插入缓冲
```

历史焦点：global → keybinding.history，**不经过 rime**。

- 焦点切换：`focus_history` 在第 4 步才触发，此时 rime 已经返回未处理。另外还要
  显式检查 `!snapshot.composing`，防止某个方案没有消费 Tab 时误切焦点。
- rime 的方案切换菜单（F4 / Ctrl+`）由 rime 自己处理，菜单以候选的形式出现，
  UI 无需特殊处理。
- 输入缓冲的 readline 键位（Ctrl+A/E/...）不会与 rime 冲突：rime 默认的
  `emacs_editing` 都带 `when: composing`，不在组字时 rime 返回未处理，按键自然落到第 4 步。
- 多行相关的键也遵循同样的规则：组字时 `ctrl+p` / `ctrl+n` / `up` / `down` 被 rime 的
  `emacs_editing` 转成 Up/Down，用来移动候选高亮；不组字时 rime 返回未处理，才作为
  `prev_line` / `next_line` 在缓冲里移动。`ctrl+j` 在 rime 默认配置里没有绑定，所以
  总是落到 `newline`；如果某个方案绑定了它，以方案为准。

### 5.2 crossterm → rime 翻译

- 字符：ASCII 用码点本身作为 keysym（`:` 就是 `colon 0x3a`，`K` 就是 `0x4b`，
  并按 X11 惯例额外带上 Shift mask）；非 ASCII 用 `0x01000000 + codepoint`。
- 特殊键映射到 X11 keysym：Return `0xff0d`、Tab `0xff09`、BackTab →
  `ISO_Left_Tab 0xfe20` + Shift、Escape `0xff1b`、BackSpace `0xff08`、Delete `0xffff`、
  Home/Left/Up/Right/Down/Prior/Next/End `0xff50..0xff57`、Insert `0xff63`、
  F1.. `0xffbe..`、Shift_L/R `0xffe1/2`、Control_L/R `0xffe3/4`、Caps_Lock `0xffe5`、
  Alt_L/R `0xffe9/a`、Super_L/R `0xffeb/c`。
- mask：Shift `1<<0`、Lock `1<<1`、Control `1<<2`、Alt(Mod1) `1<<3`、
  Super `1<<26`、Release `1<<30`。
- 大写字母：非 KKP 下 crossterm 给出 `Char('A')`（有时带 SHIFT），翻译为 keysym
  `'A'` + Shift，与 X11 行为一致。KKP 下启用 `REPORT_ALTERNATE_KEYS`，取 shifted key。
- rime 键名解析器（compat 的目标、校验用）支持 `Modifier+...+Keyname`，
  keyname 覆盖上面这张表以及 `grave`、`space`、`comma`、`period` 等标点名。

### 5.3 KKP

- `auto`：用 `crossterm::terminal::supports_keyboard_enhancement()` 查询，支持就
  push `DISAMBIGUATE_ESCAPE_CODES | REPORT_EVENT_TYPES | REPORT_ALL_KEYS_AS_ESCAPE_CODES
  | REPORT_ALTERNATE_KEYS`；退出（含 panic hook）时 pop。
- 开启后：
  - `KeyCode::Modifier(LeftShift)` 等单独修饰键的 press / release 原样发给 rime
    （release 带 Release mask），`ascii_composer` 的 Shift_L/Shift_R 切换因此可以原生使用。
  - 其它键的 release 事件也转发给 rime（rime 依靠 release 判断修饰键是否被“单独”按下），
    但 release 不触发端砚自己的动作。repeat 当作 press。
- 不管 KKP 是否开启，compat 表始终生效。复用器下查询结果不可靠时，用户可以手动 `off`。

### 5.4 compat 发送规则

- 目标是单独的修饰键（`Shift_L`、`Control_R`、`Caps_Lock` 等）：依次发送
  press(keysym, 0) 和 release(keysym, Release)，模拟一次“单独按下再松开”。
- 其它目标（如 `Control+grave`）：发送 press，再发送带 Release mask 的对应 release。

## 6. 引擎与部署

### 6.1 启动流程

1. 解析路径、加载配置、dlopen、版本检查。
2. 取实例锁（§6.3）。
3. `setup(traits)` → `set_notification_handler` → `initialize(traits)`。
4. `run_task("installation_update")`：代价很小，保证 `installation.yaml` /
   installation_id 存在（sync 需要）。
5. 按 `deploy_on_startup` 处理：
   - 首次运行（`user_data_dir/build` 下没有任何 `*.schema.yaml`）：不论策略如何都部署，
     UI 显示“首次部署中…”，并阻塞输入。
   - `notify`（默认）：在 Rust 侧做与 librime `detect_modifications` 类似的检测：取
     user 和 shared 两个目录中顶层 `*.yaml`（排除 `user.yaml`）的最大 mtime，与
     `user_config_open("user")` 读出的 `var/last_build_time` 比较。有变更就在状态栏
     显示 `⚠ 配置已变更 · F5 部署`，继续使用旧的 build。
     与 librime 的区别：**不比较目录本身的 mtime**。实测（M1）发现目录 mtime 在任何
     新建条目时都会变化，例如首次会话创建 `luna_pinyin.userdb/`，这会导致首次部署后
     的下一次启动必然误报。代价是“删除某个 yaml”检测不到，需要手动部署。
   - `auto`：用同一个检测；有变更就调用 `start_maintenance(true)` 后台部署，阻塞输入
     并显示进度。
   - `never`：跳过检测。
6. `create_session`，拿到快照，开始渲染。

注意：librime 的检测只看顶层 `*.yaml`，改动 `lua/`、`opencc/` 等子目录检测不到。
帮助页和 README 需要说明：遇到这种情况请手动 F5。

### 6.2 deploy / sync

- 动作 `deploy`：`start_maintenance(true)`，即完整部署。engine 通过 notification
  （`deploy start/success/failure`）上报状态；部署期间输入阻塞，完成后销毁并重建
  session。
- 动作 `sync`：`sync_user_data()`，同样异步，结果显示在状态栏。
- CLI：`duanyan deploy [--full]`、`duanyan sync`，不进入 TUI；日志打到终端，
  失败时返回非零退出码。
- 退出时 `destroy_session` + `finalize`，保证 userdb 落盘。

### 6.3 多实例（v1 降级方案）

- 在 `$XDG_STATE_HOME/duanyan/instance.lock` 上用 `flock` 加非阻塞排他锁。
  拿到锁的是**主实例**。
- 拿不到锁的是**从实例**：
  - 仍然正常初始化 rime，但禁用 `deploy` / `sync` 动作和启动部署，因为部署会写
    build/，不能与主实例并发。
  - 状态栏常驻提示“用户词典已被另一实例占用，本实例不学习新词”。
  - **已验证（M1）**：userdb 的 LevelDB 被锁住时，librime 只记一条
    `Error opening db ... lock ... Resource temporarily unavailable`，翻译照常，只是
    不读写用户词典。
- `ImeEngine` trait 与 `EngineCommand/EngineEvent` 是日后 daemon 化的接缝。v1 不实现 daemon。

## 7. UI

### 7.1 全屏模式（alternate screen），自上而下

1. **标题行**：`端砚-tui`、当前方案名（`get_current_schema` → schema name），右侧
   显示版本号。
2. **历史面板**（带边框，标题 `历史 · N 条`）：每条显示 `HH:MM` + 文本（非当天的条目
   改为显示 `MM-DD HH:MM`）；多行条目在列表中折叠为一行，换行显示为暗色 `↵`，
   超宽截断；被选中的多行条目展开显示全部行（最多占面板一半高度）。选中行左侧有
   竖条并高亮；最近复制的那一条右侧显示 `✓ 已复制到剪贴板`，几秒后消失。历史焦点时
   边框使用强调色。
3. **输入框**（标题 `输入`）：
   - 文本区：显示多行缓冲，preedit 内嵌在光标所在位置（已选定部分用强调色，未选部分
     正常色，并显示 rime 光标）；raw input（`get_input`）暗色显示在光标所在行的右侧。
     默认高度 1 行，随行数增长，最多占屏幕的 1/2，超出后在内部滚动，保证光标行可见。
     长行按显示宽度软折行；`prev_line` / `next_line` 按逻辑行移动（v1 不按视觉行移动）。
   - 文本区之下：候选。横排时为 `1 去哪里  2 去哪 …`，高亮项使用反色块，右侧显示
     `‹ 1/3 ›`（`is_last_page` 未知总页数时显示 `‹ 1 ›`）。竖排时每个候选一行，
     输入框随之增高。comment 用暗色显示在候选之后。
4. **状态栏**：左侧按方案 `switches` 动态生成（`get_option` + `get_state_label_abbreviated`
   / states 标签，例如 `中 │ 半角 │ 简体 │ 中文标点`；第一项即 ascii_mode，以色块突出显示）；
   鼠标点击某一项即切换对应 option。右侧显示当前焦点下的主要键位提示
   （`Tab 历史  y 复制  F1 帮助`，从当前键位配置生成），部署或降级提示也显示在这里。
5. **帮助浮层**（F1）：列出当前生效的全部键位（包括 compat），以及 KKP 状态、librime
   路径与版本、数据目录。

宽度计算统一使用 `unicode-width`；中文、emoji 与窄终端下的截断都要处理。

### 7.2 inline 模式（`--stdout`）

- `ratatui::Viewport::Inline(n)`，n = 文本区行数 + 候选行 + 状态行，单行输入时约
  3–4 行；文本区行数上限为 8，超出后内部滚动。inline viewport 高度固定，因此按上限
  预留，或在行数变化时重建 viewport（M5 时选定）。竖排候选时按页大小增高。不显示历史面板，没有历史焦点。
- crossterm 启用 `use-dev-tty` feature，事件从 `/dev/tty` 读取，界面也写到
  `/dev/tty`，这样 stdout 可以被捕获。
- 提交：恢复终端、清掉 inline 区域，把缓冲文本原样写到 stdout，然后以 0 退出。
  取消时以 1 退出。
- `--fullscreen` 强制使用全屏布局（仍然是 stdout 语义）。

### 7.3 鼠标

`tui.mouse = true` 时启用鼠标捕获：点击候选即选中该候选（`select_candidate_on_current_page`），
点击 `‹` / `›` 翻页，点击状态栏的 switch 切换它，点击历史条目选中，滚轮滚动历史。
设为 false 时保留终端原生的文本选择。

### 7.4 主题

- 内置 Catppuccin Mocha（dark）与 Latte（light），使用 truecolor。
- `mode = "auto"`：启动时发送 OSC 11 查询背景色（超时 100ms），按亮度选择
  dark 或 light；失败回落 dark。
- `[theme.colors]` 按语义键覆盖：`bg`、`surface`、`border`、`border_focus`、`text`、
  `subtext`、`accent`、`preedit`、`candidate_index`、`candidate_hl_bg`、`candidate_hl_fg`、
  `comment`、`success`、`warning`、`status_bg`。

## 8. 剪贴板

- `osc52`（默认）：写 `ESC ] 52 ; c ; <base64> BEL` 到 tty。tmux 下需要
  `set -g set-clipboard on`（写入 README）。
- `command`：把文本通过 stdin 传给外部命令。未配置 `clipboard.command` 时依次探测
  `wl-copy`（有 `WAYLAND_DISPLAY` 时）、`xclip -selection clipboard`、`xsel -b`
  （有 `DISPLAY` 时）、`pbcopy`（macOS）。
- 不使用 arboard：Linux 上剪贴板归写入它的进程所有，端砚是短命进程，退出后剪贴板
  内容就丢了；`wl-copy` 会自己 fork 一个后台进程来持有剪贴板。

## 9. 历史

- 格式为 JSON Lines，每行 `{"ts": <unix 秒>, "text": "..."}`。
- 启动时读取最后 `max_entries` 条。提交时追加一行（`O_APPEND`，一次 write，多个
  实例同时追加也安全）。`delete` 会重写整个文件（写入临时文件再 rename）。行数超过
  `2 * max_entries` 时压缩文件。
- `persist = false` 时只保存在内存里。

## 10. Nix flake

- inputs：`nixpkgs`（nixos-unstable）、`flake-utils` 或手写 `forAllSystems`
  （x86_64-linux、aarch64-linux、aarch64-darwin、x86_64-darwin）。
- `devShells.default`：
  - `rustc cargo clippy rustfmt rust-analyzer cargo-nextest`、`librime`（含头文件）、
    `rime-data`、`pkg-config`
  - 环境变量：
    - `DUANYAN_LIBRIME_PATH = "${librime}/lib/librime.so.1"`（macOS 用 `.dylib`）
    - `DUANYAN_RIME_SHARED_DIR = "${rime-data}/share/rime-data"`
    - `RIME_INCLUDE_DIR = "${librime}/include"`（给 FFI 布局测试用）
- `packages.default`：
  - 用 `rustPlatform.buildRustPackage` 构建，`cargoLock.lockFile = ./Cargo.lock`。
  - `wrapProgram --set-default DUANYAN_LIBRIME_PATH ...`。
  - 提供参数 `rimeDataPackages ? [ ]`：非空时用 `symlinkJoin` 合并出一个 rime-data，
    再 `--set-default DUANYAN_RIME_SHARED_DIR`。默认为空，也就是走自动探测。
  - `--set-default` 的语义保证用户的配置和环境变量始终优先。
- `checks`：`cargo test`、`clippy -D warnings`、`fmt --check`。

## 11. 测试

- **单元测试**：终端键表示法解析、rime 键名解析、KeyEvent → (keycode, mask) 翻译表、
  配置合并与校验（未知动作、重复绑定）、键规范化（`alt+K`/`alt+k`、`alt+:`/`alt+;`、
  `shift+k` 别名、`shift+;` 报错、非 KKP 下 `ctrl+[`≡`esc` 等别名与冲突）、多行缓冲编辑
  （含宽字符；换行插入、行间移动的目标列、行首退格合并、行尾 `kill_to_end` 删换行）、路径探测
  （注入假的 env 与文件系统）、历史 jsonl 读写与压缩、router 分发顺序（使用 mock 的
  `ImeEngine`）。
- **FFI 布局测试**：见 §3.2。
- **集成测试**（有 `DUANYAN_LIBRIME_PATH` 时才运行，devShell 默认满足）：在临时
  user_data_dir 上以 nixpkgs rime-data 部署 `luna_pinyin`，然后验证：
  - 输入 `nihao` 得到候选“你好”，空格上屏
  - 用 compat 发送 Shift_L 能切换 ascii_mode
  - 不在组字时 Tab 返回未处理
  - `detect_modifications` 的 Rust 复刻与 librime 结论一致（touch 一个 yaml 后）
  - 部署 notification 能到达 channel
- **UI 快照测试**：ratatui `TestBackend` 渲染若干固定状态，与原型比对布局。
- 手动验证清单：kitty / foot / wezterm / tmux 内 / Linux console，分别测试
  KKP auto 的判定、Shift_L 原生切换、alt+l compat、OSC52。

## 12. 里程碑

1. **M0 脚手架**：flake（devShell + package）、workspace、CI 用的 `nix flake check`。
2. **M1 rime-dl**：FFI 镜像、loader + 探测、安全封装、keysym 表、布局测试、集成测试
   （命令行 REPL 打印候选即可）。
3. **M2 引擎**：engine 线程、Command/Event、快照、notification、部署策略、实例锁、
   CLI `deploy` / `sync`。
4. **M3 按键**：终端键解析、配置键位表与合并校验、router 管线、KKP、compat。
5. **M4 全屏 UI**：各 widget、主题（含 OSC 11）、焦点、历史持久化、剪贴板、鼠标、帮助。
6. **M5 inline / --stdout**：`/dev/tty`、inline viewport、退出码。
7. **M6 收尾**：README（安装、NixOS 配置示例、tmux OSC52、KKP 说明、lua 变更需要手动部署）、
   `--print-default-config`、各终端手动验证。

## 13. 风险与待验证

- `supports_keyboard_enhancement()` 在 tmux / zellij 里的实际表现。无论结果如何
  compat 都兜底，但 auto 误判为支持时可能出现按键异常，需要在帮助页里说明手动 `off`。
- 非 KKP 下 Alt 组合键与 Esc 的歧义（crossterm 靠超时区分），会影响 `alt+l`
  这类 compat 键的可靠性。
- OSC 11 在不支持的终端上会超时；要控制启动延迟，并且不能把应答残留在输入流里。
- 手写 FFI 结构体与未来 librime 版本的 ABI 漂移，靠 `data_size` 检查和布局测试兜底。

## 14. 实现进度

实现进度、与本计划的偏差、验证情况和已知上游问题记录在同目录的
[`progress.md`](progress.md)。
