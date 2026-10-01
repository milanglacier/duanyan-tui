# bundled-frost 包计划

Releases 目前每个平台有两种包：只有二进制的包，和自带 librime 的 `-bundled`
包。两种都不带输入方案，用户还要自己 `git clone` rime-ice 或 rime-frost 到
`~/.config/duanyan/rime`。本计划新增第三种包 `-bundled-frost`：在 `-bundled`
的基础上附带固定版本的[白霜拼音](https://github.com/gaboolic/rime-frost)，作为
共享数据目录，下载解压后直接可用。

## 1. 调研结论

### 许可

- rime-frost 是 GPL-3.0（GitHub API 的 `license.spdx_id`），端砚是
  GPL-3.0-or-later，可以一起分发。
- 方案、词典和 lua 脚本都是源码形式，原样分发属于 GPL §4 的逐字复制。要做的
  事：附上它的 `LICENSE`，写明来源仓库和 tag。
- `zh-moqi.gram` 是编译出来的语言模型。上游以 GPL 发布它，构建来源是
  [gaboolic/rime-build-grammar](https://github.com/gaboolic/rime-build-grammar)，
  在附带的说明里写上这个链接。
- 词库源自雾凇拼音，雾凇的数据来源由上游负责。

### 发布物

- rime-frost 的 release 有 `rime-frost-schemas.zip`，是方案目录本身（没有
  `others/`、`.github/` 等仓库杂项）。1.0.4（2026-05-08）：
  - zip 44 MB，sha256
    `4f4998ae83f63d757c0a4ace192f69d48265bddfabe231642b73e3739ed0f2f5`；
  - 解压后 105 MB，161 个文件，自带 `LICENSE`、完整的 `opencc/`（含
    `s2t.json` 等和 `.ocd2`）、`lua/`、`zh-moqi.gram`；
  - 重新打成 tar.gz 约 44 MB，所以每个 `-bundled-frost` 包约 45–50 MB。
- 上游还有一个滚动更新的 `nightly` tag，不用它，只用带版本号的 tag。

### 白霜放在共享目录能否正常工作

librime 和插件查找各类资源时，都是先找用户目录，再找共享目录：

- 方案、配置、词典：`Service::CreateResourceResolver` 是
  `FallbackResourceResolver`，根目录是 `user_data_dir`，后备是
  `shared_data_dir`。
- lua：librime-lua 的 `lua_init`（`src/modules.cc`）把
  `<user>/lua/?.lua`、`<shared>/lua/?.lua` 都加进 `package.path`。
- 语言模型：librime-octagram 用 `CreateResourceResolver({"gram_db", "",
  ".gram"})`，同样会回退到共享目录。
- opencc：`<user>/opencc` → `<shared>/opencc`（见 bundled-opencc 计划 §1）。

本地已验证（nix 的 librime 1.17.0，白霜解压到一个目录作为
`DUANYAN_RIME_SHARED_DIR`，用户目录为空）：

- `duanyan deploy` 用时 22 秒，生成的 `build/` 有 122 MB；
- `rime-dl` 的 `repl` 示例中，输入 `rq` 出现 `2026-10-01` 等日期候选
  （`lua_translator@*date_translator`），说明 lua 从共享目录加载成功；
  `nihao` 首选「你好」。

## 2. 方案选择

| 方案 | 做法 | 问题 |
| --- | --- | --- |
| A. 新增第三种包（采用） | `-bundled` 之外再出 `-bundled-frost`，白霜放进包内 `share/rime-data` | 包的种类变成三种，需要 README 引导选择 |
| B. 白霜直接并入 `-bundled` | 仍然两种包 | 自带方案（如 rime-ice）的用户也要多下载约 40 MB；两套方案的同名文件（`cn_dicts/*`、`lua/*`、`melt_eng.*`）由用户目录优先，用户缺某个文件时会悄悄用上白霜的，难以排查 |
| C. 首次启动时联网下载 | 二进制里内置下载逻辑 | 引入网络和解压依赖、失败处理、代理问题；离线环境不可用 |
| D. 维持现状 | README 里的一行 `git clone` | 需要 git；用户的修改和上游仓库混在同一个目录里；不随端砚升级 |

采用 **A**。三种包是逐级包含的关系，不是并列选项：白霜依赖 librime-lua 和
octagram 插件，本来就只能建立在 `-bundled` 之上。

### 命名

```
duanyan-<tag>-<target>.tar.gz                 只有二进制
duanyan-<tag>-<target>-bundled.tar.gz         + librime（含插件）和 opencc 数据
duanyan-<tag>-<target>-bundled-frost.tar.gz   + 白霜拼音
```

- 用 `-bundled-frost`：在 release 资产列表里和 `-bundled` 排在一起，名字本身
  说明了「bundled 加白霜」。
- 不用 `-full` / `-batteries`：看不出带的是哪个方案。
- 不用单独的 `-frost`：看不出也带了 librime。
- 不改现有 `-bundled` 的名字，避免已有的安装脚本和文档链接失效。

## 3. 产物

```
duanyan-<tag>-<target>-bundled-frost/
  duanyan
  lib/…                    # 与 -bundled 相同
  share/
    rime-data/             # rime-frost-schemas.zip 解压的内容
      default.yaml
      rime_frost.schema.yaml …
      cn_dicts/ en_dicts/ lua/ opencc/ zh-moqi.gram …
  licenses/
    …                      # 与 -bundled 相同
    rime-frost/
      LICENSE
      SOURCE               # 仓库、tag、zip 的 sha256、zh-moqi.gram 的来源
  LICENSE
  README.md
```

- `share/rime-data/opencc`：先放 librime 构建输出的 opencc 数据（与 `-bundled`
  相同），再用白霜的 `opencc/` 覆盖，白霜自己的配置（如 `moqi_chaifen.json`、
  `emoji.json`）优先。
- 白霜 zip 根目录的 `README.md`、`LICENSE` 不留在 `share/rime-data` 里，`LICENSE`
  移到 `licenses/rime-frost/`。
- 不修改白霜的任何文件，包括 `default.yaml` 的 `schema_list`（见 §7）。

## 4. 代码改动

### `crates/duanyan/src/paths.rs`

`find_shared_data_dir` 的顺序改为：

1. 配置文件 `rime.shared_data_dir`
2. `$DUANYAN_RIME_SHARED_DIR`
3. **新增：包内的 `share/rime-data`，且其中有 `default.yaml`**（带了完整方案的包）
4. 系统候选（`$XDG_DATA_DIRS/rime-data`、`/usr/share/rime-data`、Squirrel、
   Homebrew……），顺序不变
5. 包内的 `share/rime-data`，没有 `default.yaml`（只有 opencc 的 `-bundled`
   包），现状
6. 都没有：`None`，沿用 `user_data_dir`

为什么第 3 步要排在系统目录前面：装了 fcitx5-rime / ibus-rime 的 Linux 用户都有
`/usr/share/rime-data`。如果系统目录优先，`default.yaml` 会来自系统
（朙月拼音），白霜完全不生效。用户下载 `-bundled-frost` 就是选择了白霜，应该用
包内的数据。

- 用 `default.yaml` 判断包内是否有完整方案，而不是写死「frost」：逻辑不依赖具体
  方案，以后换方案也不用改代码。
- `SharedDirSource` 增加一种来源，例如 `BundledSchemas`，与现有的 `Bundled`
  （只有 opencc）区分。
- 探测包内目录的代码（`<exe_dir>/share/rime-data`、
  `<exe_dir>/../share/rime-data`）在第 3 步和第 5 步共用，提成一个函数。
- 单元测试：
  - 包内有 `default.yaml`、系统目录也存在：选包内的，来源是 `BundledSchemas`；
  - 包内有 `default.yaml`，但配置文件或环境变量指定了共享目录：选指定的；
  - 包内没有 `default.yaml`：沿用现有 `shared_dir_bundled` 的行为；
  - `<prefix>/bin` 布局。

### `crates/duanyan/src/main.rs`

- `duanyan info`：`BundledSchemas` 显示为 `(bundled rime data)`，与现有的
  `(bundled opencc data)` 并列。
- TUI 信息面板：对应显示「包内自带的 rime 方案」，繁体界面同步。

### 影响面

- nix 包的 wrapper 设了 `DUANYAN_RIME_SHARED_DIR`，包旁边也没有
  `share/rime-data`，不受影响。
- `-bundled` 包和只有二进制的包不受影响（没有 `default.yaml`）。
- 用户目录仍然优先于共享目录：用户在 `~/.config/duanyan/rime` 里放了自己的方案
  （比如 rime-ice），他们的 `default.yaml` 生效，白霜只是占用磁盘空间。

## 5. 升级时重新部署

不改代码。CI 解压白霜时用 `unzip -DD`，不恢复 zip 里记录的时间戳，文件的 mtime
就是 CI 打包的时间。

原因：`engine::needs_deploy` 比较数据目录顶层 `*.yaml` 的 mtime 和上次部署的时间。
zip 里所有文件的时间都是白霜的发布日期（1.0.4 全是 2026-05-08）。如果保留这个
时间，用户在白霜发布之后、端砚跟进之前部署过一次，升级端砚后就不会自动重新部署，
词典还是旧的。用打包时间就不会有这个问题，因为端砚新版本发布前用户不可能已经装上
它。顶层的 `default.yaml`、`*.schema.yaml` 每次都是新的 mtime，所以每次升级端砚
都会部署一次；词典没变时 librime 按校验和跳过编译，很快就完成。

## 6. 构建与 CI（`.github/workflows/release.yml`）

- 与 librime 一样固定版本：在 `env` 里加入 `RIME_FROST_VERSION: 1.0.4` 和
  `RIME_FROST_SHA256`。
- Package 步骤，在打好 `-bundled` 包之后：
  - `gh release download "$RIME_FROST_VERSION" -R gaboolic/rime-frost -p
    rime-frost-schemas.zip`，用 `shasum -a 256 -c` 校验；
  - `cp -R "$bundled" "$frost"`，用 `unzip -DD` 把 zip 解压进
    `$frost/share/rime-data`（§5），`opencc/` 合并（白霜的覆盖包内的）；
  - 移走 `README.md`、`LICENSE`，写 `licenses/rime-frost/SOURCE`；
  - 打包，生成 `.sha256`。
- 测试 `-bundled-frost`（新步骤，与「Test the bundled librime」并列）：
  - 不设 `DUANYAN_RIME_SHARED_DIR` 和 `XDG_DATA_DIRS`，运行包内的
    `duanyan info`，断言 `shared_data_dir` 指向包内目录并带
    `(bundled rime data)`。第 3 步排在系统目录前面，所以不像 `-bundled` 的测试那样
    需要在 runner 有系统 rime 数据时跳过。
  - 空的用户目录下运行 `duanyan deploy`，断言 `build/rime_frost.table.bin` 存在，
    `stderr.log` 为空。
  - 用 `repl` 示例输入 `rq` 和 `nihao`，断言出现日期候选（lua 可用）和「你好」。
    `cargo test` 已经为该 target 编译过 `rime-dl`，`repl` 增加的编译时间很少；
    x86_64 macOS 同样在 Rosetta 下运行。
  - 每个平台多出约 25 秒的部署时间。
- Upload 步骤加入 `-bundled-frost` 的两个文件；每个 release 的资产从 16 个变为
  24 个。

## 7. 首次部署

首次启动会部署约 20 秒（本地 22 秒），因为白霜的 `schema_list` 默认启用了 8 个
方案（全拼、5 种双拼、九键、五笔、墨奇音形）。第一期不做优化：

- 不修改白霜的 `default.yaml`：用户拿到的是原样的白霜，问题可以直接对照上游
  文档排查。用户可以用 `~/.config/duanyan/rime/default.custom.yaml` 精简
  `schema_list`。
- 不附带预编译的 `build/`：librime 会把 `<shared>/build` 当作 prebuilt 数据，可以
  省掉首次部署，但 `build/` 有 122 MB，包会再大一倍左右。第一期上线后再根据反馈
  决定。
- 实现时确认 TUI 在首次部署的 20 秒里有明确的进度提示，用户不会以为程序卡住。

## 8. 文档

- README「预编译二进制」一节：
  - 表格增加 `-bundled-frost`，放在第一行，标注「推荐：下载即用」；说明三种包是
    逐级包含的关系；
  - 安装示例改用 `-bundled-frost`；
  - 原来「bundled 包不包含输入方案数据……`git clone`」一段改为：想用其他方案
    （如 rime-ice）的用户，选 `-bundled`，再 clone 方案到用户目录；
  - 新增「自定义白霜」：在 `~/.config/duanyan/rime` 里放 `*.custom.yaml`
    （如 `rime_frost.custom.yaml`、`default.custom.yaml`），不要把白霜整个复制进
    用户目录；
  - 致谢白霜拼音，说明附带的版本。
- AGENTS.md「Bundled librime」一节：加一句升级白霜时在 `release.yml` 里同时更新
  `RIME_FROST_VERSION` 和 `RIME_FROST_SHA256`。`unzip -DD` 的原因写在
  `release.yml` 的注释里。

## 9. 验证

- `nix develop -c cargo test`、`cargo clippy --all-targets`：§4 的探测顺序测试。
- `nix develop -c scripts/e2e.sh`，新增用例「bundled schemas win over system
  rime-data」：
  - 按包的布局复制二进制，旁边的 `share/rime-data` 放 `default.yaml` 和一个两
    词条的小方案（用真实的白霜会让 e2e 多 20 秒，不需要）；
  - `XDG_DATA_DIRS` 指向一个临时目录，其中的 `rime-data` 是 nix 的 rime-data，
    模拟「系统已装 rime」；
  - 断言 `duanyan info` 显示包内目录并带 `(bundled rime data)`，输入后出现小方案
    的候选；
  - 变异测试：把第 3 步移到系统候选之后，用例应失败。
- 本地手动走一遍 Linux 的 `-bundled-frost` 包：
  - 在 `ubuntu:22.04` 容器里 `apt install librime1 librime-data`（有
    `/usr/share/rime-data`），解压包，运行 `duanyan`：方案是白霜，首次部署完成后
    `rq`、`nihao` 正常，切繁体正常，`stderr.log` 为空；
  - 用户目录里放一份 rime-ice：用的是 rime-ice。
- `gh workflow run release.yml`（不填 tag），跑通四个 target，检查包的大小和
  `licenses/rime-frost/`。

## 10. 待定

- 白霜的更新节奏：上游大约每月发一个版本。默认只在发布端砚新版本时顺带升级
  白霜，不为白霜单独发版。
- 第一期上线后，根据反馈决定是否精简 `schema_list` 或附带预编译的 `build/`（§7）。
