# 作为 `$EDITOR` 使用的计划

让端砚可以设为 `EDITOR` / `VISUAL` / `core.editor`，在 git commit、`git rebase -i`、
zsh `edit-command-line`、bash `C-x C-e`、`crontab -e` 等场景里直接用 rime 编辑文件。

```sh
export EDITOR=duanyan
git commit            # 打开 .git/COMMIT_EDITMSG，打字，Enter 保存并退出
```

## 1. 现状：为什么现在不能用

调用方的约定是 `$EDITOR <file>`：编辑器读取文件、让用户修改、把结果写回同一个
文件，退出码 0 表示接受，非 0 表示放弃（git 此时报 “There was a problem with the
editor” 并中止）。端砚目前缺了三样：

1. **CLI 不接受文件参数**。`duanyan .git/COMMIT_EDITMSG` 被 clap 当成未知子命令，
   以退出码 2 失败，git 随即中止提交。
2. **不读文件**。缓冲总是从空白开始，commit 模板、rebase todo、命令行内容都看不到。
3. **不写文件**。提交只会进历史 / 剪贴板，或写到 stdout；没有“写回文件、按约定的
   退出码退出”这条路径。

另外，现有界面是为短文本设计的，直接打开文件会暴露两个问题：

- 全屏布局里输入框最多占半屏，其余给历史面板。文件可能有几十行（commit 模板、
  `commit.verbose` 下的 diff）。
- **Tab 字符没有处理**。`layout_text` 把 `\t` 当作宽度 0 的字符原样送进
  ratatui，显示错位。git 的 commit 模板里恰好有 `#\tmodified:   foo`，所以这是必修项。

## 2. 用法

```sh
export EDITOR=duanyan            # 或 VISUAL；git 也可以 git config core.editor duanyan
duanyan path/to/file             # 手动打开一个文件
```

| 形式 | 说明 |
| --- | --- |
| `duanyan <FILE>` | 编辑模式：读入文件，保存时写回 |
| `duanyan ./info` | 文件名恰好与子命令同名时，加路径前缀区分 |

`<FILE>` 与 `--stdout`、`--fullscreen`、子命令互斥，只接受一个文件。

## 3. 行为

### 3.1 打开

- 文件存在：按 UTF-8 读入，光标放在开头（commit 模板的第一行正是要写提交说明的
  位置）。
- 文件不存在：空缓冲，保存时创建（与 vim / nano 一致）。父目录不存在则保存时报错。
- 以下情况在打开终端之前报错，退出码 2：路径是目录、不可读、不是合法 UTF-8、
  大于 1 MiB（端砚不是通用编辑器，整段重排版也不适合太大的文件）。
- 行尾：文件里所有换行都是 `\r\n` 时，读入时转成 `\n`，写回时转回 `\r\n`；混合换行
  则原样保留（`\r` 按下文的控制字符显示）。开头的 UTF-8 BOM 同样读入时去掉、写回时
  加上。
- 其它内容一律原样保留，包括结尾是否有换行。

### 3.2 按键

沿用现有输入键位，只改两个动作的含义：

| 动作（默认键） | 编辑模式下 |
| --- | --- |
| `submit`（Enter） | 写回文件，退出码 0。缓冲为空也照样保存（清空 rebase todo 是合法操作） |
| `cancel`（Esc）/ 全局 `quit`（Ctrl+C） | 不写文件，退出码 1。缓冲已修改时第一次按只提示「再按一次放弃修改」，3 秒内再按才退出 |
| `newline`（Ctrl+J） | 不变，换行 |
| `focus_history`（Tab） | 不可用（与 `--stdout` 相同） |

Enter 保存而不是换行：与端砚其它模式一致，而且最常见的场景（在 commit 模板第一行
写一句提交说明）只需「打字 → Enter」。想换成其它键可以照常在 `[keybinding.input]`
里改。组字时 Enter 仍先交给 rime，这一点不变。

### 3.3 界面

- 只用全屏布局（alternate screen）。
- 不显示历史面板，输入框占满标题栏与状态栏之间的整个区域；文字超出时按现有逻辑
  滚动，保证光标可见。
- 标题栏在方案名后显示文件路径。
- 状态栏提示：`Enter 保存`、`Esc 放弃`、`Ctrl+J 换行`、`F1 帮助`。
- 帮助页的输入框一节注明编辑模式下 submit / cancel 的含义。

### 3.4 显示控制字符（所有模式）

- `\t` 显示为空格，对齐到下一个 8 列制表位（与终端和 git 一致）。显示时按所在的
  显示行计算，软换行后从行首重新计算；放不下时与其它字符一样折到下一行。
- 其它控制字符（`\r`、`\x1b` 等）显示为 `^M` 形式的脱字符记法，绝不把原始控制字节
  写进终端。
- `Buffer` 的列计算（`col`、`offset_at_col`，决定上下移动时的目标列）改用同一套
  显示宽度函数，保证上下移动与显示对齐。

粘贴进来的 Tab 也受益于这一改动。

### 3.5 与其它功能的关系

- **历史**：编辑模式的内容不记入历史，保存时也不复制到剪贴板。文件里往往带着模板
  注释或 diff，记进历史只是噪音。
- **多实例**：从全屏端砚里跑 `git commit` 会再启动一个端砚，它按现有规则成为次实例
  （出候选但不学习新词），状态栏照常提示。
- **部署**：首次运行照常部署，行为不变。

## 4. 实现

### 4.1 CLI（`main.rs`）

```rust
/// Edit FILE and write the result back (for use as $EDITOR).
#[arg(value_name = "FILE", conflicts_with_all = ["stdout", "print_default_config"])]
file: Option<PathBuf>,
```

加 `#[command(args_conflicts_with_subcommands = true)]`，避免 `duanyan file deploy`
这类组合被接受。`run` 在 `command` 为 `None` 时把 `file` 传给 `tui`。

### 4.2 文件读写（新模块 `edit.rs`）

```rust
pub struct EditFile {
    pub path: PathBuf,
    /// Text as loaded into the buffer (line endings normalized, BOM removed).
    pub original: String,
    crlf: bool,
    bom: bool,
}

impl EditFile {
    pub fn load(path: PathBuf) -> anyhow::Result<Self>;
    /// Restores CRLF / BOM and overwrites the file in place.
    pub fn save(&self, text: &str) -> std::io::Result<()>;
}
```

- 读取与格式检测拆成纯函数 `decode(&[u8]) -> Result<(String, crlf, bom)>` 和
  `encode(&str, crlf, bom) -> Vec<u8>`，便于单元测试。
- 写回用「打开 + 截断 + 写入」原地覆盖，不用「写临时文件再 rename」：保留 inode、
  权限、属主和符号链接，`crontab -e`、`sudoedit` 等依赖这些的调用方不受影响。
  `$EDITOR` 打开的几乎都是调用方的临时文件，原地写中途失败的风险可以接受。

### 4.3 App

- 把 `stdout_mode: bool` 换成
  `enum Mode { Scratch, Stdout, Edit }`，现有判断 `stdout_mode` 的地方逐一改写，
  `FocusHistory` 在 `Stdout` 和 `Edit` 下都不可用。
- `App` 增加 `edit_original: Option<String>` 用于判断「已修改」；启动时把
  `EditFile::original` 插入缓冲并把光标移回开头（`Buffer` 增加 `set_text`）。
- `submit()` 在 `Edit` 下：允许空缓冲；不写历史、不复制；推入
  `Effect::Save(String)`。
- 事件循环处理 `Effect::Save`：调用 `EditFile::save`，成功则
  `app.exit = Some(Exit::Saved)`，失败则 `notify(Error, "保存失败：…")` 并留在界面里，
  用户可以处理权限等问题后重试，文字不会丢。在界面内保存（而不是退出后再写）就是为了
  这一点。
- `cancel` / `quit` 在 `Edit` 下：未修改直接 `Exit::Cancel`；已修改时记一个
  `discard_armed: Option<Instant>`，在 `MESSAGE_TTL` 内再次触发才退出。
- `Exit` 增加 `Saved`；`tui()` 结尾的退出码映射为 `Saved → 0`、`Cancel → 1`。

### 4.4 UI

- `UiContext` 增加 `edit_path: Option<String>`，标题栏显示。
- `draw_fullscreen` 在编辑模式下跳过历史面板，输入框高度取整个 body。
- `draw_status` 的提示按 `Mode` 选择（3.3）。
- 控制字符显示（3.4）：抽出 `fn cell_width(g: &str, col: usize) -> usize` 与
  `fn display(g: &str, col: usize) -> Cow<str>`，`layout_text` 和 `Buffer` 共用。

### 4.5 devShell

`packages` 加入 `git`，供 e2e 的 git commit 场景使用。

## 5. 测试

单元测试：

- `edit.rs`：LF、CRLF、混合换行、BOM、无结尾换行、空文件的 decode/encode 往返；
  非 UTF-8 报错；文件不存在得到空缓冲。
- `buffer.rs`：含 Tab 的行上下移动保持目标列；Tab 在不同起始列的宽度。
- `ui.rs`：`layout_text` 把 `\t` 展开到制表位、控制字符显示为脱字符，光标列正确。
- `app.rs`（用现有的 Fake engine）：编辑模式下空缓冲可保存；未修改时 Esc 直接取消；
  已修改时第一次 Esc 只提示、第二次才取消；Tab 不切到历史；保存不写历史。

`scripts/e2e.sh` 新增：

- 在 shell 窗格里 `printf 'hello\n#\tcomment\n' > f; duanyan f; echo code=$?`，输入
  `nihao ` 后 Enter：`code=0`，`cat f` 为 `你好hello` 加原来的第二行；打开时屏幕上
  Tab 显示为对齐的空格。
- 修改后按一次 Esc 看到提示，再按一次：`code=1`，文件内容不变。
- 不存在的文件：保存后被创建；非 UTF-8 文件：直接以 `code=2` 退出并在 stderr 报错。
- git：临时仓库里 `GIT_EDITOR=duanyan git commit --allow-empty`，输入并 Enter 后
  `git log -1 --format=%s` 为输入的中文；Esc 放弃时 git 报错且没有新提交。
- 只读文件（`chmod 444`）：Enter 后状态栏显示「保存失败」，界面不退出。
- 结束时 `stderr.log` 为空。

手动：kitty 中把 `EDITOR=duanyan` 用于 zsh `edit-command-line` 和
`git rebase -i`，确认编辑后的内容被调用方正确读回。

## 6. 文档

README「使用」一节加一小段「作为编辑器」：`export EDITOR=duanyan`、Enter 保存、
Esc 放弃（非零退出码）、Ctrl+J 换行；命令表加入 `duanyan <FILE>`。

## 7. 不做

- **行号参数 `+N`**（`less` 的 `v` 等会传）：首版不解析，`duanyan +N file` 会因为多出
  一个参数而报错退出；需要时再加。
- **多个文件**：只接受一个。
- **inline 界面编辑文件**：三行的 inline 区域放不下模板，编辑模式只用全屏。
- **大文件优化**：`layout_text` 每帧重排整段文字，1 MiB 以内够用；需要编辑更大的
  文件时再改为只排可见区域。
- **自动剥离 git 注释行**：由 git 自己处理（`commit.cleanup`），端砚原样读写。
