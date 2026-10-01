# 进度

分支 `bundled-frost`。

## 已完成

- 调研（2026-10-01）：许可、发布物内容与体积、librime 和插件对共享目录的回退；
  本地用白霜作为共享目录部署（22 秒）并验证 lua、opencc 可用。结论见 plan.md §1。
- `paths.rs`：`SharedDirSource` 分为 `BundledSchemas`、`BundledOpencc`；包内
  `share/rime-data` 有 `default.yaml` 时排在配置文件和环境变量之后、系统目录之前。
  新增单元测试 `shared_dir_bundled_schemas`。
- `main.rs`：`duanyan info` 标 `(bundled rime data)`，TUI 信息面板标「包内自带的
  rime 方案」（繁体「套件內附的 rime 方案」）。
- `scripts/e2e.sh` 新增用例「bundled: schemas next to the binary win over system
  rime data」：包内放一个小方案，`XDG_DATA_DIRS` 指向 nix 的 rime-data 模拟系统
  数据，用户目录为空。变异测试：去掉 `default.yaml` 的优先判断后，三项断言全部
  失败（显示的是朙月拼音）。
- `release.yml`：固定 `RIME_FROST_VERSION`、`RIME_FROST_SHA256`；Package 步骤打
  `-bundled-frost` 包（`unzip -DD`，白霜的 `opencc/` 覆盖 librime 的，`LICENSE`
  移到 `licenses/rime-frost/` 并写 `SOURCE`）；新增「Test the bundled rime-frost」
  步骤（`info`、`deploy` 的 stderr、`build/rime_frost.table.bin`、`repl` 输入
  `rq` / `nihao`）；上传新包。actionlint 通过（顺带把写 `GITHUB_OUTPUT` 的几行
  合成一组，消除 SC2129）。
- 本地模拟：从 workflow 里原样取出打包和测试脚本，用 nix 的 librime 拼出 bundled
  目录后运行，全部通过；包约 56 MB（含 debug 二进制和 nix 的 librime）。
- README：三种包的表格（`-bundled-frost` 置顶推荐）、安装示例、自定义白霜、
  目录表、许可证说明。AGENTS.md：升级白霜时要改的位置，以及优先级的原因。
- `cargo test`、`cargo clippy --all-targets`、`scripts/e2e.sh` 全部通过。
  `cargo fmt --check` 只报 `app.rs` 里一处已有的格式问题，与本次改动无关，未改。

- release workflow（不填 tag）：第一次运行两个 macOS target 失败，原因是 macOS 版
  librime 的插件在 glog 初始化前往 stderr 写 INFO 日志，测试把它们当成了部署错误；
  改成只认 glog 的 ERROR / FATAL 行后，第二次运行
  （https://github.com/milanglacier/duanyan-tui/actions/runs/36922786404）四个
  target 全部通过。aarch64 macOS 的 `-bundled-frost` 包 49 MB（`-bundled` 5.7 MB），
  内含 `licenses/rime-frost/{LICENSE,SOURCE}`、`zh-moqi.gram` 和 `rime-plugins/`。

## 待做

- 合并到 main，把本计划移到 `completed/`。
- 另行处理：macOS 上用 bundled 包启动 TUI 时，上面那几行 INFO 日志大概也会写进
  `stderr.log`（现有 `-bundled` 包的问题，与本计划无关，未验证）。
