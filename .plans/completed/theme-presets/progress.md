# 主题预设：实现进度

设计见 [`plan.md`](plan.md)。

## 状态

全部完成。用户试看后把 Rosé Pine 的主色从 foam / pine 改为 iris，标题改用
foam / pine。

- `config.rs`：`ThemePreset`（`kebab-case`），`ThemeSection.preset`。
- `theme.rs`：`ROSE_PINE_MOON`、`ROSE_PINE_DAWN`、`TOKYO_NIGHT`、`TOKYO_DAY`、
  `SOLARIZED_DARK`、`SOLARIZED_LIGHT` 与 `Theme::preset()`。
- `main.rs`：按 `preset` 和深浅选底色，再套 `[theme.colors]`。
- `default_config.toml`、README 加上 `preset`。

## 验证

- `cargo test`、`cargo clippy --all-targets` 通过；新增 `theme::tests::presets`、
  `config::tests::theme_preset_values`。
- `scripts/e2e.sh` 全部通过，新增 “theme preset from the config”：
  Tokyo Night 深色版的前景、背景色出现在屏幕上。
