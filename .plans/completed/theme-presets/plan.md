# 主题预设：Rosé Pine、Tokyo Night、Solarized

现在只有 Catppuccin 一套配色：`theme.mode` 在 Mocha（深色）和 Latte（浅色）之间
选择，`[theme.colors]` 逐项覆盖。本计划再加三套预设，并新增配置项
`theme.preset` 选择其中一套。

## 1. 配置

```toml
[theme]
preset = "catppuccin"   # catppuccin | rose-pine | tokyo-night | solarized
mode = "auto"           # auto | dark | light
```

- `preset` 选配色家族，`mode` 仍然只管深浅，两者正交。每套预设都有深浅两个
  变体：

  | preset        | dark          | light          |
  | ------------- | ------------- | -------------- |
  | `catppuccin`  | Mocha         | Latte          |
  | `rose-pine`   | Moon          | Dawn           |
  | `tokyo-night` | Night         | Day            |
  | `solarized`   | Solarized Dark| Solarized Light|

- 默认 `catppuccin`，写进 `default_config.toml`，现有用户的外观不变。
- 未知的预设名由 serde 报错（`ThemeSection` 已经 `deny_unknown_fields`，枚举
  用 `kebab-case`），和 `mode` 写错时一样。
- `[theme.colors]` 覆盖的是选中预设、选中深浅之后的结果，语义不变。

## 2. 实现

- `config.rs`：新增 `ThemePreset` 枚举（`Catppuccin`、`RosePine`、`TokyoNight`、
  `Solarized`），`ThemeSection` 加 `preset` 字段。
- `theme.rs`：
  - 新增六个常量 `ROSE_PINE_MOON`、`ROSE_PINE_DAWN`、`TOKYO_NIGHT`、`TOKYO_DAY`、
    `SOLARIZED_DARK`、`SOLARIZED_LIGHT`，与 `MOCHA`、`LATTE` 并列。
  - 新增 `Theme::preset(ThemePreset, light: bool) -> Theme`。
- `main.rs`：`let base = if light { LATTE } else { MOCHA }` 改为
  `Theme::preset(cfg.theme.preset, light)`。启动前对 `[theme.colors]` 的检查不依赖
  预设，保持不变。

## 3. 配色映射

各个槽位按 Catppuccin 的用法对应到各家调色板：一种主色（`accent`、
`border_focus`、`preedit`、`candidate_hl_bg`），另一种色做标题，`candidate_hl_fg`
取背景色，`status_bg` 比背景略深或略浅一级，`selection_bg`、`key_bg` 用该家族的
高亮底色。

- **Rosé Pine**（取自 rose-pine/neovim 的 `palette.lua`）：主色 iris（Rosé Pine
  的终端配色把它当作 magenta）；标题 Moon 用 foam、Dawn 用 pine（Dawn 的 foam 在
  浅底上太淡）；成功 leaf（Rosé Pine 没有绿色）、警告 gold、错误 love；
  `border` 用 highlight_high，`selection_bg`、`key_bg` 用 highlight_med；
  `status_bg` Moon 用 surface，Dawn 用 overlay（Dawn 的 surface 比 base 更亮，
  几乎看不出区别）。
- **Tokyo Night**（取自 folke/tokyonight.nvim `extras/lua` 的 night / day）：主色
  blue，标题 magenta，`subtext` 用 dark5，`comment`/`candidate_index` 用 comment，
  `border` 用 fg_gutter，`status_bg` 用 bg_dark，`selection_bg` 用 bg_visual。
- **Solarized**（Ethan Schoonover 的 16 色）：主色 blue，标题 violet。深色版正文
  base0、`subtext` base00、`comment`/`border` base01，底色层 base02；浅色版对称地
  用 base00、base0、base1 和 base2。

## 4. 测试

- `theme.rs` 单元测试：六个新变体与 `preset()` 的对应关系；每个变体的
  `candidate_hl_fg` 等于 `bg`（与 Catppuccin 一致的约定）。
- `config.rs` 单元测试：默认是 `catppuccin`；四个合法值都能解析；未知值报错。
- `scripts/e2e.sh`：新增一步，用 `--config` 指定
  `preset = "tokyo-night"`、`mode = "dark"`，用 `capture-pane -e` 确认正文是
  Tokyo Night 的前景色 `#c0caf5`、背景是 `#1a1b26`。
- `cargo test`、`cargo clippy --all-targets`、`scripts/e2e.sh` 全部通过。

## 5. 文档

- `default_config.toml`：加 `preset` 及取值注释。
- README 配置一节的 `[theme]` 示例加上 `preset`，表格里 “深浅色与配色” 改为
  “配色预设、深浅色与逐项配色”。
- `AGENTS.md` 无需补充。
