# 端砚 Duanyan v1 实现进度

设计见 [`plan.md`](plan.md)。本文件记录实现状态、与计划的偏差和验证情况。

## 里程碑

| 里程碑 | 状态 | 提交 |
| --- | --- | --- |
| M0 脚手架（flake、workspace） | 完成 | `61b7c0d` |
| M1 rime-dl（FFI、loader、keysym、布局测试、集成测试） | 完成 | `61b7c0d` |
| M2 引擎（快照、部署策略、实例锁、CLI deploy / sync） | 完成 | `c0710af` |
| M3 按键（键表示法、键位表、按键管线、KKP、compat） | 完成 | `c0710af` |
| M4 全屏 UI（主题、焦点、历史、剪贴板、鼠标、帮助） | 完成 | `c0710af` |
| M5 inline / `--stdout` | 完成 | `c0710af` |
| M6 收尾（README、`--print-default-config`） | 完成 | `c0710af` |
| 端到端测试（`scripts/e2e.sh`）与 `AGENTS.md` | 完成 | `8f6ad12` |

各终端的手动验证尚未全部完成，见「待验证」。

## M1 实测发现（已并入计划正文）

- glog 的 `log_dir` 必须预先创建，否则每条日志都往 stderr 报错（计划 §3.2）。
- userdb 被另一进程锁住时，librime 只记一条错误，翻译照常（计划 §6.3）。
- librime 的 `detect_modifications` 会比较目录本身的 mtime，首次会话创建 userdb 后
  必然误报；Rust 侧的检测只看顶层 `*.yaml`（计划 §6.1）。

## 与计划的偏差

- **没有独立的 engine 线程**。librime 的部署本来就在它自己的维护线程里异步进行，
  所以主线程直接调用 librime，每个 tick 轮询 `is_maintenance_mode()`，完成后重建
  会话。daemon 化的接缝保留为 `ImeEngine` trait 加拥有所有权的 `ImeSnapshot`。
- **`--version --verbose` 改为 `duanyan info` 子命令**。
- **inline 模式用 `Viewport::Fixed`**，文本区固定预留 3 行，超出后内部滚动。
  ratatui 的 `Viewport::Inline` 通过 crossterm `cursor::position()` 查询光标，
  而它把查询写到 stdout，会被 `$(...)` 吞掉；改为自己在 `/dev/tty` 上发 DSR
  查询光标行。KKP 与 OSC 11 探测同理，都手写在 `/dev/tty` 上
  （crossterm 的 `supports_keyboard_enhancement()` 也写 stdout）。
- **glog 会把 ERROR 级别日志抄送 stderr**，与 `log_dir` 无关（`stderrthreshold`
  默认为 ERROR）。TUI 模式在加载 librime 前设置 `GLOG_stderrthreshold=3`，并把
  fd 2 重定向到 `log/stderr.log`，退出时恢复。
- **不再单独调用 `run_task("installation_update")`**。deployer 模块在
  `start_maintenance` 之前尚未加载，单独调用会报 unknown task；部署和同步也会自己
  运行这个任务。
- **配置错误按动作路径报告**（如 `keybinding.input.submit: ...`），不给行号：
  用户配置先与默认值合并再反序列化，行号信息在合并时丢失。
- **键位**：历史面板额外加了 `first`（g / home）和 `last`（G / end）；`next` / `prev`
  也绑定了 ctrl+n / ctrl+p。默认值以 `crates/duanyan/src/default_config.toml` 为准。
- **帮助页**支持 j/k、PageUp/PageDown 滚动，按其它键关闭。

## 已验证

- 单元测试 42 个；rime-dl 的 FFI 布局测试和真实 librime 集成测试；`nix build`
  的 check 阶段在沙箱里也会跑集成测试。
- 在 tmux（传统编码）里手动验证了：
  - 首次部署、打字与候选、多行输入
  - OSC 52 写入 tmux 缓冲区
  - alt+l compat 切换中英
  - 历史焦点、取回、F1 帮助
  - `--stdout` 的输出与退出码（0 / 1）
  - 已有端砚在运行时，新开的端砚降级（不学习新词）
  - `notify` 提示与 F5 部署
- 用户在 kitty 中手动验证了鼠标点击（候选、状态栏开关、历史条目）。
- `scripts/e2e.sh` 在 tmux 中自动复现上述 tmux 手动验证的主要流程。

## 待验证

- 在 foot / WezTerm / Ghostty 中实测 KKP。kitty（不经复用器）已由用户确认：KKP
  生效，单独 Shift 可以切换中英。
- Linux console。
- macOS（Squirrel 的 librime 与 SharedSupport 探测）。

## 已知上游问题

- **herdr 不转发单独的修饰键**（herdr `0d5d6f1`，2026-09-27）。
  - herdr 支持 KKP：回答 `CSI ? u` 查询，向外层终端申请 event types，并按每个 pane
    申请的 flags 重新编码按键。所以端砚的 auto 探测判定为支持，这一判定本身没错。
  - 但它的编码函数 `try_encode_csi_u`（`src/input/encode.rs`）只处理字符键和
    Enter / Tab / 方向键等功能键。`KeyCode::Modifier` 落到 `_ => return None`，回退到
    传统编码后得到空字节，Shift_L / Shift_R 的按下与松开被直接丢弃。因此在 herdr 里
    单按 Shift 不能切换中英。
  - 端砚无法绕过也无法检测：herdr 对 pane 的查询如实返回已启用的 flags。
  - 应对：在 herdr 中使用 compat 映射（默认 alt+l / alt+r），它不依赖 KKP，已验证可用。
  - 上游修复方向：pane 开启 REPORT_ALL_KEYS_AS_ESCAPE_CODES 时，为 `KeyCode::Modifier`
    按 kitty 协议输出功能键编码（LeftShift 57441、RightShift 57447 等）。
