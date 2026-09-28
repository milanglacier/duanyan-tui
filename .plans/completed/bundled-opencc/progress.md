# 进度

分支 `bundled-opencc`。

## 已完成

- `paths.rs`：`find_shared_data_dir` 返回 `(PathBuf, SharedDirSource)`，在所有
  系统候选之后加入 `<exe_dir>/share/rime-data`、`<exe_dir>/../share/rime-data`；
  新增 `shared_dir_bundled` 单元测试。
- `main.rs`：`duanyan info` 在包内目录后标 `(bundled opencc data)`，TUI 信息面板
  标「bundled 包自带的 opencc 数据」。
- `scripts/build-librime.sh`：opencc 的 `PKGDATADIR` 改为 `/usr/share/opencc`
  （通过 `CMAKE_CXX_FLAGS`，不改源文件），并自检。在本地 `ubuntu:22.04` 容器中
  增量构建通过，产物里只有 `/usr/share/opencc/`，没有构建路径。
- `release.yml`：bundled 包加入 `share/rime-data/opencc`；冒烟测试断言
  `shared_data_dir` 指向包内目录（runner 上有系统 rime 数据时跳过并告警）；
  integration 测试改用包内的 opencc 数据。actionlint、shellcheck 通过。
- README、AGENTS.md 不改：包内的 opencc 数据是实现细节，不面向用户；代码和
  脚本里的注释已足够说明。整个仓库跑了一次 `cargo fmt`（顺带格式化了 `app.rs`
  的一处旧代码）。
- `scripts/e2e.sh` 新增用例「bundled: opencc data next to the binary is the
  last-resort shared data」：
  - 按 bundled 包的布局把二进制复制到临时目录，经软链接启动，旁边放
    `share/rime-data/opencc/`；不设 `DUANYAN_RIME_SHARED_DIR`，`XDG_DATA_DIRS`
    指向空目录。系统里有 rime 共享目录（`/usr/share/rime-data` 等）时跳过。
  - nix 的 librime 动态链接 nix 的 opencc，自己就能找到 `s2t.json`，测不出数据
    来自哪里；所以包内只放一个标记配置 `e2e_marker.json`（文本词典，把「你好」
    转成「包内标记」）。用户目录里放 prelude 和一个两词条的小方案
    `e2e_bundled`，`simplifier` 引用这个标记配置并默认开启。
  - 断言：`duanyan info` 显示包内目录并带 `(bundled opencc data)`；输入
    `nihao` 出现「包内标记」；该实例的 `stderr.log` 为空。
  - 变异测试：把 `paths.rs` 里的 `share` 改坏后，前两项断言失败，说明用例能抓到
    回归。
  - librime 编译只有一个词条的 table 会报 `invalid metadata`，所以小方案的词典
    放了两条。
- `cargo test`、`cargo clippy --all-targets`、`cargo fmt --check` 通过；
  `scripts/e2e.sh` 全部通过（需要用不签名的 git 配置跑，否则 git commit 那两项
  会因全局配置的 gpg 签名超时失败，与本次改动无关）。

## 手动验证

- NixOS 宿主机，无系统 rime 共享目录（`XDG_DATA_DIRS` 指向空目录）：本地拼出的
  bundled 目录经软链接启动，`duanyan info` 显示包内共享目录；rime-ice 以繁体
  模式启动，`zhongguo` 候选为「中國」「種過」，`stderr.log` 为空。
  - 对照：移走包内 `share/` 后候选变回「中国」，并报
    `opencc config not found: s2t.json`。
- `ubuntu:22.04` 容器，`apt install librime1 librime-data`（系统 librime 1.7.3，
  没有 `/usr/share/rime-data/opencc`）：`duanyan info` 选中
  `/usr/share/rime-data`；bundled librime + rime-ice 繁体模式输出「中國」，
  strace 显示 `s2t.json` 与 `.ocd2` 从 `/usr/share/opencc` 加载，日志无错误。

## 与计划的出入

- `PKGDATADIR` 用 `CMAKE_CXX_FLAGS` 覆盖，而不是 `sed` 修改 OpenCC 的
  `CMakeLists.txt`。
- Debian/Ubuntu 的 `.ocd2` 由 `libopencc1.1` 安装时生成；只装了
  `libopencc-data`、没装 `libopencc1.1` 时 `/usr/share/opencc` 下只有 json，
  会找到 `s2t.json` 但缺词典。正常装了 librime 的系统不会出现这种情况。

## 分层测试

| 验证内容 | 位置 |
| --- | --- |
| duanyan 选中包内目录，librime 从中读取 opencc 配置 | e2e（标记配置） |
| 静态 librime 的 `PKGDATADIR` 为 `/usr/share/opencc` | `build-librime.sh` 自检 |
| 发布产物在干净环境里使用包内 opencc | CI 冒烟测试与 integration 测试 |
| 静态 librime + 系统 rime → `/usr/share/opencc` | 目前只做了本地容器手动验证；可选在 CI 的 Linux job 中 `apt install librime1 librime-data` 后用 `DUANYAN_RIME_SHARED_DIR=/usr/share/rime-data` 跑 integration 测试（未做） |

## 收尾

- `gh workflow run release.yml --ref bundled-opencc`（不填 tag）四个 target 均
  通过（run 36490845818），随 v0.1.4 发布。
- 未验证：macOS 上没装 Squirrel 时 rime-ice 的繁体切换，需要在 Mac 上手动确认。
