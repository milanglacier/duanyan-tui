# Emacs 按键与输入框鼠标支持

三件事：补几个 Emacs 风格的编辑键；把切换中英文的 compat 默认键从 `alt+l` /
`alt+r` 换成 `ctrl+l` / `ctrl+r`；全屏模式下输入框支持鼠标点击定位光标、拖动选中
自动复制、Backspace 删除选区。

## 1. Emacs 按键

`[keybinding.input]` 新增四个动作，和其它 input 动作一样只在 rime 不处理该键时
生效，help 页面按 `InputAction` 的顺序自动列出：

| 动作 | 默认键 | 行为 |
| --- | --- | --- |
| `word_left` | `alt+b` | 光标移到前一个词的开头 |
| `word_right` | `alt+f` | 光标移到后一个词的结尾 |
| `buffer_start` | `alt+<` | 光标移到整个缓冲区开头 |
| `buffer_end` | `alt+>` | 光标移到整个缓冲区结尾 |

- **词的划分与 `ctrl+w`（`Buffer::kill_word`）一致**：UAX #29 分词，每个汉字单独
  是一个词；跳过空白（不含换行）；换行单独算一步。`kill_word` 的前向扫描抽成
  `word_start_before(cursor)`，`kill_word` 与 `word_left` 共用；`word_right` 用
  对称的 `word_end_after(cursor)`。
- `alt+<` / `alt+>`：`<`、`>` 是带 shift 的符号，`KeySpec::from_event` 对字符键
  不保留 SHIFT，老终端的 `ESC <` 和 KKP 的 alternate key 都会归一成 `alt+<`。
  单元测试覆盖这两种事件。
- `alt+n` / `alt+p` 暂不做。

## 2. compat 默认键

```toml
[keybinding.compat]
Shift_L = "ctrl+l"
Shift_R = "ctrl+r"
```

- 直接替换 `alt+l` / `alt+r`：不保留旧键，不写兼容代码，也不写测试去断言旧键
  失效。这是全局默认值，inline 模式也会跟着变。
- compat 在 rime 之前匹配，所以 `ctrl+l` / `ctrl+r` 不会再发给 rime；老终端也能
  发出 `ctrl+小写字母`。
- 要同步修改的地方：README（中英文切换一段）、`config.rs` 的 `defaults_build` /
  `user_overrides_merge_per_action` 测试、`app.rs` 的 `compat_sends_shift_tap`、
  `scripts/e2e.sh` 里的 “compat alt+l toggles ascii mode” 一步。

## 3. 输入框鼠标支持（仅全屏）

只在全屏模式启用，包括编辑文件模式；inline（`--stdout`）模式点击输入区的行为
保持不变。`tui.mouse = false` 时整体不生效（不开启鼠标捕获）。

### 3.1 坐标到缓冲区偏移

`layout_cells` 排版时给每个格子记下对应的 buffer 字节偏移（preedit 格子没有），
产出每个可见行的 `(起始列, 宽度, 字节偏移)` 列表和该行末尾的偏移。`draw_fullscreen`
把它连同文本区 `Rect`、滚动偏移放进 `HitMap::text`。

`TextHits::grapheme_at` 按 `(列, 行)` 找到对应的字符，返回它前后的偏移：

- 落在某个格子上：该字符。宽字符和 Tab 的左右两半没有区别。
- 落在行尾之后：该可视行末尾的空范围。自动折行产生的行，末尾指向下一行首字符
  之前。
- 落在最后一行下面（文本区内的空白行）：文本末尾的空范围。

### 3.2 滚动位置稳定

现在 `render_text` 每帧用 `crow - (h - 1)` 算滚动偏移，光标在底部时点击上方某行
会让整个视图跳动。改为：沿用上一帧的偏移（从 `app.hits.text` 读），只在光标出界
时滚到刚好可见。不新增 `App` 字段。inline 模式也用同一逻辑，光标上移时视图不再
强制把光标行贴在底部。

### 3.3 点击

左键在文本区按下：

- 正在输入（`snapshot.composing`）时忽略，和点击历史记录的规则一样。
- 否则光标一律落在被点字符的左边（最初按左右半边区分，用起来不好预料，已改掉），
  焦点回到输入框，清除旧选区。

点在输入框的边框或候选区域，行为和现在一样。

### 3.4 拖动选中与复制

- `App` 新增 `selection: Option<Range<usize>>` 和按下时记录的 `drag_anchor`（按下
  处的字符）。和终端原生选择一样，起点和终点下的字符都包含在选区内，往哪个方向
  拖都一样；光标跟着终点走（向后拖到终点字符之后，向前拖到终点字符之前）。拖出
  文本区上下边界时，终点卡在可见范围的首行 / 末行，不自动滚动。
- `Up(Left)`：选区非空时推送 `Effect::Copy(选中文本)`，状态栏提示
  “已复制”；复制失败时沿用现有的“复制失败：…”。
- 高亮保留到下一次按键、粘贴或点击。按键时先取出选区：若该键最终落到 input 表的
  `backspace` 动作（rime 未处理），删除选区、光标到选区开头；其它键照常执行，选区
  消失。rime 在 release 事件里上屏文字时也清除选区，避免偏移失效。
- 高亮复用主题已有的 `selection_bg`（历史面板选中行用的颜色）：`layout_cells`
  给选区内的格子叠加这个背景。
- `EnableMouseCapture` 已开启按键拖动上报（1002）和 SGR（1006），不用改 `tty.rs`。
- 多数终端按住 Shift 拖动会绕过鼠标捕获、使用终端自己的选择，这一点不受影响。

## 4. 测试

- `buffer.rs`：`word_left` / `word_right` 覆盖英文、中文、空白、换行、首尾边界。
  `kill_word` 沿用现有测试。
- `keys.rs` / `config.rs`：`alt+<` 的两种事件形式；新的 compat 默认键。
- `ui.rs`：`layout_cells` 的偏移映射覆盖宽字符、Tab、自动折行、光标在行尾折行、
  preedit 在中间；滚动偏移的沿用与出界修正。
- `app.rs`：用伪造的 `HitMap::text` 测点击定位、composing 时忽略、拖动选区、松开
  产生 `Effect::Copy`、Backspace 删除选区、其它键清除选区、空选区不复制。
- `scripts/e2e.sh`：
  - 把 compat 那一步改成 `ctrl+l`，并加一步 `alt+b` / `alt+f`、`alt+<` / `alt+>`
    的光标移动（输入后在光标处插入字符来验证位置）。
  - 用 `send-keys -l` 注入 SGR 鼠标序列（`\e[<0;x;yM`、`\e[<32;x;yM`、
    `\e[<0;x;ym`）验证点击、拖动复制和 Backspace 删除选区，复制结果看
    `tmux show-buffer`。

## 5. 文档

- README：更新中英文切换的默认键；在输入框按键说明里补上新的四个键；说明全屏模式
  下可以点击定位、拖动复制、Backspace 删除选区，以及 Shift+拖动仍是终端原生选择。
- `default_config.toml`：新增动作和新的 compat 默认键。
- AGENTS.md：记录用 tmux 注入 SGR 鼠标序列的做法，删掉“鼠标点击无法用脚本测”。
