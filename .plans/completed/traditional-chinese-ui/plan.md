# 繁体中文界面

TUI 界面文字支持繁体（台湾用语）。默认根据环境变量判断：繁体中文环境显示繁体，
其余一律简体。配置文件可以强制指定。

## 1. 范围

翻译端砚自己画出来的全部文字：

- 标题栏（`端砚-tui` → `端硯-tui`）、输入框和历史面板的标题、历史为空时的提示、
  “已复制到剪贴板”标记；
- 状态栏的按键提示、部署 / 同步中、配置已变更、从实例警告；
- 所有 `notify` 通知（`app.rs`、`main.rs`）；
- 帮助页：分组标题、动作名称、“未绑定”“当前终端不可用”、环境信息的标签和值、
  底部提示；
- clap 的 `about`（`--help` 第一行）。

不翻译：

- CLI 子命令（`deploy`、`sync`、`info`、`init`）的输出、配置和键位的校验错误、
  clap 的其余帮助文字，这些原本就是英文；
- 通知里拼接的系统错误信息（`{e}`）；
- rime 提供的文字：方案名、状态栏的中 / 西等开关标签、候选词。想要繁体候选应
  选用繁体方案，端砚不做转换；
- README 和 `default_config.toml` 的注释。

## 2. 配置

`[tui]` 新增：

```toml
language = "auto"                           # auto | simplified | traditional
```

`auto` 按第 3 节检测，另外两个值直接指定，不看环境变量。

## 3. 检测

只看环境变量，不读 macOS 的系统语言设置：SSH 会转发这些变量，本机和远程的结果
一致，也不需要新增依赖。

1. 按 POSIX 的优先级，取 `LC_ALL`、`LC_MESSAGES`、`LANG` 中第一个非空的值作为
   消息语言。
2. 按 gettext 的规则处理 `LANGUAGE`：它非空，并且第 1 步的结果不是 `C` 或
   `POSIX` 时，依次查看它用冒号分隔的每一项，取第一个 `zh` 开头的项。例如
   `LANGUAGE=en_US:zh_TW` 判定为繁体，因为端砚只有中文界面。
3. 没有从 `LANGUAGE` 中取到值时，使用第 1 步的结果。

判断一个 locale 值：去掉 `.编码` 和 `@修饰符`，按 `_` 或 `-` 切分，不区分大小写：

- 语言不是 `zh` → 简体；
- 带文字标签时由它决定：`Hant` → 繁体，`Hans` → 简体（`zh_Hans_HK` 为简体）；
- 没有文字标签时看地区：`TW`、`HK`、`MO` → 繁体，其余（`CN`、`SG`、没有地区）
  → 简体。

都没有设置、值无法解析，或者 locale 是 `C` / `POSIX` 时，用简体。

检测写成一个纯函数，环境变量从参数传入（复用 `paths.rs` 里读环境变量的抽象），
方便做表驱动的单元测试。

`--help` 在读取配置之前就打印了，`about` 只按环境变量检测，不看 `language`
配置项。

## 4. 实现

新增 `i18n.rs`：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang { Simplified, Traditional }

impl Lang {
    pub fn detect(env: &impl Env) -> Self { ... }
    /// 简繁两种写法并排写在调用处。
    pub fn tr(self, simplified: &'static str, traditional: &'static str) -> &'static str { ... }
}
```

- 每处文字改为 `lang.tr("部署完成", "部署完成")`。简繁两种写法并排放在调用处，
  review 时不用在文件之间来回对照，漏写繁体也无法编译。不引入 fluent / gettext
  这类框架，也不用 OpenCC 在运行时转换，因为台湾用语无法逐字转换。
- 带参数的文字用 `format!` 拼接，例如
  `format!("{}{e}", lang.tr("写入历史失败：", "寫入歷史紀錄失敗："))`。
  有两处参数在中间（`配置已变更 · {key} 部署`、`历史 · {n} 条`），在调用处按
  语言写两个 `format!`。
- `Lang` 在 `main.rs` 读完配置后确定，存进 `App`（和 `copy_on_submit` 一样作为
  字段），`ui.rs` 通过 `app.lang` 读取。`UiContext.info` 在 `main.rs` 构建时
  已经知道语言。
- `action_label` 增加 `lang` 参数，每个动作写两种名称。

## 5. 繁体用语对照

以台湾用语为准，港澳用户也能读懂。主要词汇：

| 简体 | 繁体 |
| --- | --- |
| 端砚 | 端硯 |
| 配置文件 / 配置已变更 | 設定檔 / 設定已變更 |
| 文件 | 檔案 |
| 剪贴板 | 剪貼簿 |
| 复制 / 已复制选中文字 | 複製 / 已複製選取的文字 |
| 保存 | 儲存 |
| 提交 | 送出 |
| 放弃 | 放棄 |
| 撤销 / 重做 | 復原 / 重做 |
| 历史（面板标题） / 条 | 歷史紀錄 / 筆 |
| 用户词典 | 使用者詞典 |
| 用户数据目录 / 共享数据目录 | 使用者資料目錄 / 共用資料目錄 |
| 日志目录 | 記錄檔目錄 |
| 运行 | 執行 |
| 维护任务 | 維護工作 |
| 键盘协议 | 鍵盤協定 |
| 当前终端不可用 | 目前終端機不支援 |
| 全局 | 全域 |
| 滚动 | 捲動 |
| 检测 | 偵測 |
| 文本开头 / 文本结尾 | 文字開頭 / 文字結尾 |
| 自带 | 內附 |

rime、kitty keyboard protocol、bundled、compat、lua/、opencc/ 这些专有名词
保持原样。

## 6. 测试

- `i18n.rs` 单元测试：表驱动覆盖第 3 节的规则，包括 `zh_TW.UTF-8`、`zh_HK`、
  `zh_MO`、`zh_CN`、`zh_SG`、`zh`、`zh-Hant`、`zh_Hant_TW`、`zh_Hans_HK`、
  `en_US`、`C`、`POSIX`、全部为空、`LC_ALL` 覆盖 `LANG`、`LC_MESSAGES` 覆盖
  `LANG`、`LANGUAGE=en_US:zh_TW`，以及 locale 为 `C` 时忽略 `LANGUAGE`。
- `config.rs`：`language` 的默认值、三个合法值，以及拒绝非法值。
- `ui.rs`：用 ratatui 的 `TestBackend` 画一次繁体界面，确认标题栏和按键提示是
  繁体。
- `scripts/e2e.sh`：
  - 现有用例在启动命令里清掉 `LANGUAGE`、`LC_ALL`，并设置
    `LC_MESSAGES=zh_CN.UTF-8`，这样开发者本机的 locale 不会影响结果；
  - 新增一个 `LANG=zh_TW.UTF-8` 的全屏会话，检查 `端硯-tui`、按键提示和
    “已複製到剪貼簿”；
  - 新增一个 `LANG=zh_TW.UTF-8`、配置 `language = "simplified"` 的会话，检查
    显示的是简体。

## 7. 文档

- README 的配置部分加一行说明 `language` 的作用和默认的检测规则。
- `default_config.toml` 加 `language = "auto"`。
- AGENTS.md 不改：`lang.tr(简, 繁)` 的写法看代码就知道；繁体用台湾用语写在
  `i18n.rs` 的模块注释里；e2e 固定 locale 环境变量的原因写在 `e2e.sh` 的注释里。
