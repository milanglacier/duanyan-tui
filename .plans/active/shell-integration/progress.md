# Shell 集成实现进度

设计见 [`plan.md`](plan.md)。

## 状态

全部完成，已提交到 `shell-integration` 分支。

- `duanyan init <zsh|bash|fish>`，脚本在 `crates/duanyan/src/shell/`。
- inline 界面：光标不在行首时从下一行开始画，退出后光标回到原来的行和列
  （`tty::place_inline`，纯函数，有单元测试）。
- 修复滚屏：先把光标移到最后一行再写换行。
- devShell 加入 zsh、fish、bashInteractive。
- README 新增简短的「Shell 集成」一节。

## 与计划的偏差

- fish 的回退固定为 `commandline -f complete`，不去解析 `bind \t` 的原绑定：
  输出格式随版本变化（`\t` / `tab`、`--preset`），解析不可靠。
- 没有预编辑：`^^<Tab>` 总是打开空白的端砚（最终设计，见 plan 第 6 节）。

## 已验证

- `cargo test`、`cargo clippy --all-targets` 通过。
- `scripts/e2e.sh` 新增并通过：
  - zsh、fish：`^^` + Tab 打开端砚时提示符行保留；提交后触发串被替换、光标在插入
    文字之后、命令能执行；Esc 取消后命令行不变；没有触发串时 Tab 照常补全。
  - bash：`bind -x` 绑定后在含中文的行中间插入，`READLINE_POINT` 按字符计算正确
    （bash 5.3）。
  - `--stdout` 在屏幕靠下（非最后一行）打开时，上方输出不被覆盖。用改动前的代码跑
    这一项会失败，确认了原来的滚屏问题。
- 用户在自己日常使用的 zsh 里手动验证：`^^` + Tab 打开端砚，提交的文字正确插入命令行。
- e2e 注意：NixOS 的 `/etc/zshenv` 即使 `zsh -f` 也会读取并重置 `PATH`，
  所以 zsh 窗格里要重新把 wrapper 目录加到 `PATH`。

## 待验证

- 手动：kitty 中 zsh 多行提示符、RPROMPT 下的重绘。
- fish vi 模式（`bind -M insert`）。
