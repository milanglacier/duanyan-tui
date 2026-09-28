# bundled 包附带 opencc 数据计划

`-bundled` 包目前只带 librime，不带 opencc 词典。本计划在包内附带 opencc 数据，
作为**最低优先级**的共享数据目录：系统有 rime 共享目录时仍用系统的，找不到时才
用包内的。

## 1. 调研结论

### rime-ice / rime-frost 是否自带 opencc

- **rime-frost**：`opencc/` 下有完整的 OpenCC 配置和词典（`s2t.json`、
  `t2s.json`、`s2tw.json` 等，以及 `STPhrases.ocd2` 等 `.ocd2`）。放进
  `user_data_dir` 后，librime 优先查用户目录，所以不需要包内的数据。
- **rime-ice**：`opencc/` 下只有 `emoji.json`、`emoji.txt`、`others.txt`。
  但各方案的 `traditionalize` 写的是 `opencc_config: s2t.json`（注释说明为
  「Rime 内置配置，在程序目录可找到」），依赖前端提供 `s2t.json`。bundled 包 +
  rime-ice 时，切换到繁体会失败（`opencc config not found`，写入 glog ERROR，
  也就进了 `stderr.log`）。
- 所以 bundled-librime 计划 §7 里「这两套方案……不依赖 opencc」对 rime-ice 不
  成立，README 也要改。

### librime 1.17.0 查找 opencc 配置的顺序

`src/rime/gear/simplifier.cc`（`SimplifierComponent::Create`），相对路径的
`opencc_config` 依次尝试：

1. `<user_data_dir>/opencc/<name>`
2. `<shared_data_dir>/opencc/<name>`
3. 都不存在时，把裸文件名交给 OpenCC 的 `Config::NewFromFile`，OpenCC 再查
   （`deps/opencc/src/Config.cpp`，`FindConfigFile`）：
   1. 当前工作目录；
   2. 编译期写死的 `PKGDATADIR`（= OpenCC 的 `${CMAKE_INSTALL_PREFIX}/share/opencc`）。

`.ocd2` 词典在 json 所在目录里找，所以每个 `opencc/` 目录必须自成一套。

第 3 步对 bundled 包没用：Linux 版的 `PKGDATADIR` 是 CI 上的构建路径，macOS 官方
包同理；OpenCC 也没有环境变量可以改它。能控制的只有 **shared_data_dir 是哪个
目录**。

### Linux 的系统共享目录通常没有 opencc 配置

发行版的 librime 动态链接系统的 libopencc，`s2t.json` 等由 OpenCC 自己的数据包
放在 `/usr/share/opencc/`，librime 走上面的第 3 步（libopencc 的 `PKGDATADIR`）
找到。`/usr/share/rime-data/opencc` 一般不存在。已核实：

- Debian / Ubuntu：没有任何包提供 `rime-data/opencc`；`s2t.json` 在
  `libopencc-data` 的 `/usr/share/opencc/`，librime 依赖 `libopencc1.1`，后者
  依赖 `libopencc-data`。
- Arch：`librime`、`librime-data`、`rime-prelude`、`rime-essay`、
  `rime-luna-pinyin`、`fcitx5-rime` 都不带 opencc 配置；`librime` 依赖
  `opencc`，由它提供 `/usr/share/opencc/s2t.json`。
- Fedora：librime 以 `opencc-devel` 构建，同样动态链接。
- nixpkgs：`rime-data` 的 `opencc/` 只有 emoji 和 `t2hkf.json`；`s2t.json` 在
  `opencc` 包的 `share/opencc`，librime 同样动态链接。
- fcitx5-rime / ibus-rime 本身不带 opencc 数据，完全靠 librime → libopencc。
  自带 opencc 数据的是静态链接 librime 的前端：Squirrel（`SharedSupport/opencc`）、
  Weasel、Trime。

## 2. 方案选择

用户设想的「按文件回退：系统 share 优先，包内 opencc 兜底」在 librime 的查找链
里没有插入点：librime 只看一个 shared_data_dir，之后的查找由 OpenCC 按编译期
路径进行。可行的做法：

| 方案 | 做法 | 问题 |
| --- | --- | --- |
| A. 目录级回退（推荐） | `find_shared_data_dir` 在所有系统候选之后加入包内的 `share/rime-data`（只含 `opencc/`） | 找到了系统共享目录、但它缺 `s2t.json` 时用不上包内数据（见 §5） |
| B. 往用户目录补文件 | 启动时把用户目录和共享目录都没有的 opencc 文件复制进 `user_data_dir/opencc` | 往用户的 rime 目录（常是 rime-ice 的 git clone）写文件；复制后优先级反而高于系统；升级 bundled 包后要同步 |
| C. 合成 overlay 目录 | 在 state 目录里把系统共享目录和包内 opencc 拼成软链接目录，作为 shared_data_dir | 实现复杂，要跟踪系统目录的变化，调试困难 |
| D. 修改 OpenCC 的 `PKGDATADIR` | 编译时改成包内路径 | 只能是绝对路径，无法相对于库的位置；macOS 用官方包，改不了 |

采用 **A**：不改 librime、不写用户目录，逻辑和现有的 librime 探测对称
（包内 `lib/` → 包内 `share/`）。A 覆盖不到的「Linux 已装系统 rime」情况，由
§5 在 Linux 构建里把 OpenCC 的 `PKGDATADIR` 设为固定的 `/usr/share/opencc`
补上（这是 D 的变体：不指向包内，而是指向发行版 opencc 数据的固定位置）。

## 3. 产物

```
duanyan-<tag>-<target>-bundled/
  duanyan
  lib/…
  share/
    rime-data/
      opencc/          # OpenCC 配置（*.json）和词典（*.ocd2），约 1.1 MB
  licenses/            # opencc 的许可证已在其中
  LICENSE
  README.md
```

`share/rime-data/opencc` 的内容就是 `scripts/build-librime.sh` 已经输出的
`share/opencc`（OpenCC 从源码生成），与平台无关；macOS 包继续从 x86_64 Linux 的
artifact 取。目录名用 `rime-data`，与 `/usr/share/rime-data` 的布局一致，
`<prefix>/{bin,lib,share}` 安装方式也适用。

## 4. 代码改动：`crates/duanyan/src/paths.rs`

`find_shared_data_dir` 的顺序：

1. 配置文件 `rime.shared_data_dir`
2. `$DUANYAN_RIME_SHARED_DIR`
3. 系统候选（`$XDG_DATA_DIRS/rime-data`、`/usr/share/rime-data`、Squirrel、
   Homebrew……），顺序不变
4. **新增：`<exe_dir>/share/rime-data`、`<exe_dir>/../share/rime-data`（存在才
   加入）**
5. 都没有：`None`，沿用 `user_data_dir`

- 返回值区分来源，让 `info` 和 TUI 的信息面板能标出「包内」：
  `find_shared_data_dir` 返回 `Option<(PathBuf, SharedSource)>` 或类似的小结构，
  `Setup::shared_detected: bool` 换成来源枚举。`duanyan info` 显示
  `shared_data_dir  <path> (bundled)`；`None` 时的
  `(none found; using user_data_dir)` 保留。
- 单元测试 `shared_dir_bundled`：只有包内目录时选它；包内和系统都在时选系统；
  `<prefix>/bin` 布局。
- 影响面检查：
  - 以前没有共享目录时 `shared_data_dir == user_data_dir`，现在变成一个只含
    `opencc/` 的只读目录。librime 部署时 `default.yaml`、方案文件都会在用户目录
    找到（rime-ice / rime-frost 都自带），与「系统 rime-data 只装了 opencc」
    等价。
  - `data_dirs()` 会多一个目录参与 `needs_deploy` 的 mtime 比较；包内目录不会
    变，最多在升级 bundled 包后触发一次部署。
  - nix 包的 wrapper 设了 `DUANYAN_RIME_SHARED_DIR`，且包旁边没有
    `share/rime-data`，不受影响。

## 5. Linux：把静态 OpenCC 的 `PKGDATADIR` 设为 `/usr/share/opencc`

只做 §4 的话，Linux 上装了 fcitx5-rime / ibus-rime 的用户（有
`/usr/share/rime-data`、没有 `rime-data/opencc`）又用 bundled 包时（比如
Ubuntu 22.04 的 librime 1.7.3 低于要求的 1.8），会选中系统共享目录，包内数据用
不上；静态链接的 OpenCC 的 `PKGDATADIR` 又是构建路径，也找不到
`/usr/share/opencc`。按 §1 的核实，这是 Linux 上装了 rime 时的**常见情况**，
不是边角情况。

所以 Linux 构建时把 OpenCC 编译期的 `PKGDATADIR` 改成 `/usr/share/opencc`，让
bundled 的 librime 和发行版的 librime 查找方式一致：

- 装了系统 rime：`<user>/opencc` → `/usr/share/rime-data/opencc`（通常不存在）
  → `/usr/share/opencc`（发行版的 opencc 数据包，librime 的依赖，基本一定在）。
- 没装系统 rime：`<user>/opencc` → 包内 `share/rime-data/opencc`。

做法：OpenCC 的 `PKGDATADIR` 和数据安装目录都来自 `DIR_SHARE_OPENCC`，唯一的
开关 `SHARE_INSTALL_PREFIX` 会把词典装到构建机的 `/usr/share`，所以不用它。
`scripts/build-librime.sh` 只给 opencc 传
`-DCMAKE_CXX_FLAGS="$CXXFLAGS -UPKGDATADIR -DPKGDATADIR='\"/usr/share/opencc\"'"`：
CMake 的编译命令是 `<DEFINES> <INCLUDES> <FLAGS>`，这里的 `-U`/`-D` 排在
`add_definitions` 之后，覆盖原值且不报重复定义；不修改任何源文件，数据仍装进
`$prefix/share/opencc`。构建后自检：库里必须有 `/usr/share/opencc/`，不能有
`$prefix/share/opencc`。

剩余限制：

- macOS 用官方预编译包，改不了 `PKGDATADIR`。装了 Squirrel 时用它的
  `SharedSupport`（自带 opencc），不受影响；只有「Homebrew 的
  `/opt/homebrew/share/rime-data` 存在、又用 bundled 包」这种少见组合会缺
  `s2t.json`。
- README 给出通用的解决办法：用 rime-frost（自带 opencc），或把包内的
  `share/rime-data/opencc` 复制到 `~/.config/duanyan/rime/opencc`。

## 6. 构建与 CI

- `scripts/build-librime.sh`：按 §5 修改 `PKGDATADIR` 并自检；头部注释里
  `share/opencc`「仅供测试」改为「随 bundled 包发布」。脚本变了，CI 会重建一次
  librime（缓存 key 是脚本的哈希）。
- `.github/workflows/release.yml` 的 Package 步骤：
  `mkdir -p "$bundled/share/rime-data"` 并
  `cp -R librime-linux/share/opencc "$bundled/share/rime-data/opencc"`；
  更新注释。
- 冒烟测试：
  - 在不设 `DUANYAN_RIME_SHARED_DIR` 的环境里跑包内的 `duanyan info`，断言
    `shared_data_dir` 指向 `$BUNDLED/share/rime-data` 并带 `(bundled)`（GitHub
    的 runner 没有 `/usr/share/rime-data`；断言前先确认这一点，否则跳过并告警）。
  - `rime-dl` integration 测试拼共享目录时，opencc 改从
    `$BUNDLED/share/rime-data/opencc` 复制，这样 `luna_pinyin_simp` 的 t2s 转换
    实际用的是发布出去的文件。

## 7. 文档

包内的 opencc 数据是实现细节，不写进 README 和 AGENTS.md；代码和脚本的注释已
足够说明。只在 `.plans/active/bundled-librime/plan.md` §1 关于 opencc 的结论处
加注，指向本计划。

## 8. 验证

- `nix develop -c cargo test`、`cargo clippy --all-targets`：新增的探测顺序测试。
- `nix develop -c scripts/e2e.sh`：确认没有回归（e2e 设了
  `DUANYAN_RIME_SHARED_DIR`，走不到新分支）。
- 本地手动走一遍 Linux bundled 包：用已有的 `ubuntu:22.04` 构建产物拼出 bundled
  目录，在不设 `DUANYAN_RIME_SHARED_DIR` 的 tmux 会话里（`XDG_DATA_DIRS` 指向
  空目录，避开 NixOS 的系统 rime-data），放入 rime-ice：
  - `duanyan info` 显示包内共享目录；
  - 输入后用 rime-ice 的繁体开关切到繁体，候选变成繁体；
  - `stderr.log` 为空，rime 日志里没有 `opencc config not found`。
- 在 `ubuntu:22.04` 容器里模拟「已装系统 rime」：`apt install librime1
  librime-data`（有 `/usr/share/rime-data`、没有其中的 `opencc/`），用 bundled
  包 + rime-ice 切繁体，确认 `s2t.json` 从 `/usr/share/opencc` 加载。注意
  Debian/Ubuntu 的 `.ocd2` 不在 `libopencc-data` 里，而是 `libopencc1.1` 安装时
  生成，所以要按真实依赖装 `librime1`，只装 `libopencc-data` 会缺词典。
- `gh workflow run release.yml`（不填 tag）跑通四个 target。
