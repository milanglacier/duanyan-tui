# 进度

分支 `bundled-librime`。

## 已完成

- `paths.rs`：`Env::exe_dir`，librime 候选在 `$DUANYAN_LIBRIME_PATH` 之后、系统
  库之前加入 `<exe_dir>/lib/` 与 `<exe_dir>/../lib/`；新增 `librime_bundled`
  单元测试。
- `scripts/build-librime.sh`：在本地 `ubuntu:22.04` 容器（x86_64）中从零构建
  成功。产物 NEEDED 只有 `libm.so.6 libc.so.6 ld-linux-x86-64.so.2`，最高
  GLIBC 2.34，约 7 MB。
  - 在宿主机（NixOS）上用该 `.so` 跑 `cargo test -p rime-dl`：layout 与
    integration（部署 luna_pinyin_simp、输入「你好」）均通过。
- 按 README 走了一遍 Linux bundled 包：二进制经软链接启动，`duanyan info`
  显示包内 `lib/librime.so.1 (1.17.0)`；rime-ice 放进
  `~/.config/duanyan/rime` 后首次启动自动部署（约 14 秒），`nihao` 出候选，
  `rq` 出日期（lua 插件工作），`stderr.log` 为空。
- `release.yml`：`librime-linux` job、bundled 打包、冒烟测试、`tag` 留空时只
  上传 workflow artifact、main 上的 `push` 触发预热缓存。actionlint 通过。
- README、AGENTS.md 已更新。
- `cargo test`、`cargo clippy --all-targets`、`scripts/e2e.sh`、shellcheck
  通过。

## 与计划的出入

- 脚本只接收 `<outdir>`，版本号固定在脚本里。
- 依赖不用 `deps.mk`，自己调用 CMake，以便关掉 unwind、snappy 等可选系统库。
- opencc 词典生成需要 python3。
- `dry_run` 输入改为「`tag` 留空」。
- macOS 的 `licenses/` 和测试用 opencc 数据取自 x86_64 Linux 的 artifact。

## 待办

- 推送分支后用 `gh workflow run release.yml --ref bundled-librime` 跑一次（不填
  tag），确认四个 target 都通过，尤其是：
  - aarch64 Linux 的构建（本地只验证了 x86_64）；
  - x86_64-apple-darwin 在 Rosetta 下的测试。
- macOS 插件加载（§4 第 3 点）需要在 Mac 上手动确认：rime-ice 的 `rq` 能出
  日期，rime 日志中没有 `error loading plugin`。
