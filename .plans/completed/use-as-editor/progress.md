# 作为 `$EDITOR` 使用：实现进度

设计见 [`plan.md`](plan.md)。

## 状态

全部完成。

- `duanyan <FILE>`：与 `--stdout`、`--print-default-config`、子命令互斥
  （`args_conflicts_with_subcommands`）。
- `edit.rs`：读入（不存在为空、目录 / 非 UTF-8 / 超过 1 MiB 报错）、CRLF 与 BOM
  的还原、原地写回。
- `App`：`stdout_mode` 换成 `Mode { Scratch, Stdout, Edit }`；`Effect::Save`
  由事件循环写文件，成功后 `Exit::Saved`（退出码 0），失败时状态栏报错、不退出；
  已修改时 Esc / Ctrl+C 需要按两次，中间按了其它键就重新计。
- UI：编辑模式隐藏历史面板、输入框占满中间区域；标题栏显示文件路径，太长时从左侧
  截断保留文件名；状态栏提示保存 / 放弃。
- 所有模式：`buffer::cell` 统一决定显示宽度，Tab 展开到 8 列制表位，控制字符显示为
  `^M` 形式；`Buffer` 的上下移动用同一套宽度。CRLF 在排版时拆成两格，仍然换行。
- devShell 加入 `git`；README 新增「作为编辑器」一小节。

## 与计划的偏差

- 计划里的 `cell_width` / `display` 合成一个 `buffer::cell(g, col) -> (Cow<str>, usize)`；
  `edit_original` 是 `String`（只在 `Mode::Edit` 下有意义），不是 `Option`。
- 帮助页标签改为「提交 / 保存」「取消 / 清空 / 放弃」，标签列宽由 16 改为 18。
- 标题栏的路径从左侧截断：git 传入的是 `.git/COMMIT_EDITMSG` 的绝对路径，
  不截断会盖住右侧的版本号（手动截屏时发现）。

## 已验证

- `cargo test`、`cargo clippy --all-targets` 通过。`cargo fmt --check` 只剩
  `app.rs` 里原有的一处（`start()` 中的 notify），与本次改动无关。
- `scripts/e2e.sh` 新增并通过：Tab 显示对齐；打字 + Enter 保存，退出码 0，文件
  内容正确；修改后 Esc 先提示、再按退出码 1、文件不变；不存在的文件保存时创建；
  只读文件保存失败时状态栏报错、界面不退出；非 UTF-8 文件直接以 2 退出；
  `GIT_EDITOR=duanyan git commit` 提交中文说明，Esc 放弃时 git 中止、没有新提交。
  结束时 `stderr.log` 为空。
- 手动在 tmux 里用真实的 git commit 模板截屏：`#\tnew file:` 对齐，历史面板隐藏。
- 用户手动验证了 zsh `edit-command-line`。

## 已知问题

- 有一次整轮 e2e 中，原有的 bash widget 步骤超时（`bind -x` 刚执行完就发键，疑似
  竞态），随后连续两轮全部通过；与本次改动无关，未处理。

## 不再验证

- `git rebase -i`：用中文输入法编辑 rebase todo 的场景很少，用户决定不测。
