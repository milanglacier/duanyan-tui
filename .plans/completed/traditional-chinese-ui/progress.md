# 进度

## 已完成

- `i18n.rs`：`Lang`、`tr`、按环境变量检测，含表驱动单元测试。
- `[tui] language`，含配置单元测试和 `default_config.toml`。
- `app.rs`、`main.rs`、`ui.rs` 的界面文字全部改为 `lang.tr(简, 繁)`；clap
  `about` 在运行时按环境变量设置。
- `scripts/e2e.sh`：包装脚本固定 locale；新增繁体界面、配置覆盖和 `--help` 的检查。
- README 配置部分说明 `language`。

## 与计划的出入

- `Lang::detect` 接受一个读环境变量的闭包，而不是 `paths::Env`：`Env` 还要求
  `exists`、`exe_dir`，检测用不到。
- 没有加 `TestBackend` 的渲染测试：`ui.rs` 没有现成的渲染测试，fake engine
  只在 `app.rs` 的测试里，渲染由 e2e 覆盖。
- e2e 用 `LC_MESSAGES` 而不是 `LANG` 切换 locale，经 `env` 传给端砚：在 bash
  包装脚本里直接赋值 `LC_*`，bash 会因为本机没有该 locale 打印警告。
- 帮助页动作名一栏从 18 列加宽到 20 列：`取消 / 清空 / 放弃` 正好 18 列，
  和按键之间没有空格，简体也有这个问题。

## 验证

- `cargo test`、`cargo clippy --all-targets`、`cargo fmt --check` 通过。
- `scripts/e2e.sh` 全部通过。前两次运行中 bash / fish 集成各失败过一次，
  原因是按键在 shell 执行完设置命令之前到达，与本改动无关，重跑通过。
