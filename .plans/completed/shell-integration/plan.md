# Shell 集成计划

类似 fzf 的 `**<Tab>`：在命令行里输入 `^^` 再按 Tab，就地打开端砚 inline 界面，
提交的文字替换掉 `^^` 插入到命令行光标处。

```sh
git commit -m "^^<Tab>      # 打开端砚，提交「修复拼写」后
git commit -m "修复拼写      # 光标停在插入文字之后
```

## 1. 用法

```sh
# ~/.zshrc
eval "$(duanyan init zsh)"
# ~/.bashrc
eval "$(duanyan init bash)"
# ~/.config/fish/config.fish
duanyan init fish | source
```

| 变量 / 函数 | 作用 |
| --- | --- |
| `DUANYAN_TRIGGER` | 触发串，默认 `^^`；设为空串则关闭 Tab 触发 |
| zsh `duanyan-widget` | ZLE widget，可自行 `bindkey` 到任意键 |
| bash `__duanyan_widget` | 供 `bind -x` 绑定 |
| fish `duanyan_widget` | 供 `bind` 绑定 |

默认只接管 Tab（zsh / fish），不额外占用其它键。

## 2. 行为

- 光标左侧以触发串结尾时按 Tab：打开 `duanyan --stdout`。
  - 提交：删掉触发串，把文字插入光标处，光标移到插入文字之后。
  - 取消（Esc / Ctrl+C，退出码 1）或出错（退出码 2）：命令行保持原样，触发串保留。
- 否则按 Tab 执行原来的补全（初始化时记下 Tab 原先绑定的 widget，
  例如 `expand-or-complete` 或 fzf 的 `fzf-completion`），与 fzf 的 `**` 可以共存，
  加载顺序不限。
- 文字原样插入，不做 shell 转义：用户通常在引号里触发，转义反而出错。多行文字
  （Ctrl+J）照样插入，zsh / fish 的命令行本身支持多行。

## 3. 实现

### 3.1 CLI

新增子命令 `duanyan init <zsh|bash|fish>`，把内嵌的脚本打印到 stdout。脚本放在
`crates/duanyan/src/shell/duanyan.{zsh,bash,fish}`，用 `include_str!` 编入二进制，
与 `default_config.toml` 的做法一致。子命令名沿用 zoxide / starship 的 `init` 惯例。

### 3.2 inline 界面：光标不在行首时保留当前行

现在 `tty::reserve_inline` 从光标所在行开始画界面。从 shell 执行
`out=$(duanyan --stdout)` 时光标在行首，没有问题；但从 widget 里调用时光标停在提示符
那一行的中间，界面会盖住提示符和已输入的命令。

改为与 fzf `--height` 相同的做法：

- 用 DSR 同时取光标的行和列（`cursor_row` 改为 `cursor_position`）。
- 列 > 0 时，界面从下一行开始；列 = 0 时维持现状，从当前行开始。
- 退出时清空界面区域，并把光标放回**原来的行和列**（按滚屏行数修正行号），shell
  随后重绘命令行即可对齐。列 = 0 时这与现在「停在区域左上角」的结果相同。

顺带修一个现有问题：需要滚屏时，现在的代码从光标所在行写 N 个换行，但光标不在最后
一行时，前几个换行只是下移而不滚屏，实际滚动行数少于 N，界面会盖住光标上方的内容。
改为先把光标移到最后一行再写换行。

### 3.3 zsh

```zsh
__duanyan_tab() {
  local trigger=${DUANYAN_TRIGGER-'^^'} text
  if [[ -n $trigger && $LBUFFER == *"$trigger" ]]; then
    text=$(command duanyan --stdout) && LBUFFER=${LBUFFER%"$trigger"}$text
    zle reset-prompt
  else
    zle $__duanyan_tab_fallback
  fi
}
```

- 初始化时从 `bindkey '^I'` 取原 widget 存进 `__duanyan_tab_fallback`；若已经是
  `__duanyan_tab`（重复 source）则不覆盖。
- 绑定到 `emacs` 和 `viins` 两个 keymap。

### 3.4 fish

`bind \t` 到一个函数：`commandline -cb`（光标左侧）以触发串结尾就调用端砚并用
`commandline -i` 插入，否则 `commandline -f complete`。最后 `commandline -f repaint`。
vi 模式另外绑定 `-M insert`。

### 3.5 bash：只提供 widget，不接管 Tab

bash 的 `bind -x` 函数无法回退到 readline 的 `complete`。唯一的办法是把 Tab 绑成宏
「先跑 shell 函数、再跑 complete」，但这样 readline 记录的上一个命令变成了那个函数，
**连按两次 Tab 列出候选**的行为会失效（`rl_complete` 靠 `rl_last_func == rl_complete`
判断第二次 Tab）。fzf 的 bash 版改走 `complete -F` 可编程补全，要按命令逐个注册、
兼容 bash-completion，代码量大且容易与用户已有的补全冲突。

因此 bash 只定义 `__duanyan_widget`（通过 `READLINE_LINE` / `READLINE_POINT`
插入），README 给出绑定示例：

```bash
bind -x '"\C-x\C-d": __duanyan_widget'
```

不默认占用任何键。

### 3.6 devShell

`packages` 加入 `zsh`、`fish`、`bash`（`bashInteractive`），供 e2e 使用。

## 4. 测试

- 单元测试：DSR 回复解析出行和列；`reserve_inline` 的区域与恢复位置计算抽成纯函数
  测试（列 = 0 / 列 > 0 / 需要滚屏 / 界面高度占满屏幕）。
- `scripts/e2e.sh` 新增（均在 `-f /dev/null` 的干净 shell 里 source 集成脚本）：
  - zsh：输入 `echo ^^` 按 Tab → 打字提交 → 命令行变成 `echo 你好`，提示符未被覆盖；
    回车执行后屏幕输出 `你好`。
  - zsh：Esc 取消后命令行仍是 `echo ^^`。
  - zsh：没有触发串时 Tab 仍能补全（例如 `ech<Tab>` → `echo`）。
  - fish：同上三项。
  - bash：`bind -x` 绑定示例键后插入文字，检查多字节字符下光标位置正确
    （`READLINE_POINT` 与字符下标一致）。
  - 在屏幕中部（非最后一行）触发，确认上方已有输出没有被界面覆盖。
- 手动：在 kitty 中验证 zsh 多行提示符、右侧提示符（RPROMPT）下重绘正常。

## 5. 文档

README 新增简短的「Shell 集成」一节：三种 shell 的启用方式、`DUANYAN_TRIGGER`、
bash 的绑定示例（一句话说明不接管 Tab）。命令表加入 `duanyan init <shell>`。
README 面向用户，保持简洁，实现细节只写在本计划里。

## 6. 不做

- **触发串前的文字不作为初始输入**：fzf 会把 `**` 前的词当查询，端砚不这样做。
  `^^<Tab>` 总是打开一个空白、没有预编辑的端砚，这是最终设计。
- bash 的 Tab 触发（见 3.5）。
