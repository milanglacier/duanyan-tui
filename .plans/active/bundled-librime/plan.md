# 随包附带 librime 计划

Release 除了现有的纯二进制包，再为每个 target 提供一个 `-bundled` 包，内含
librime 1.17.0。二进制启动时优先探测包内的 librime，解压即可用，不再依赖系统
安装的 librime。

## 1. 调研结论

- librime 1.17.0 的 release 只提供 **macOS universal** 和 Windows 的预编译库，
  **没有 Linux**。
- macOS 包 `rime-33e7814-macOS-universal.tar.bz2`：
  - `dist/lib/librime.1.17.0.dylib`（x86_64 + arm64），依赖只有 `libSystem` 和
    `libc++`；boost、glog、leveldb、marisa、opencc、yaml-cpp 都已静态链接。
  - install name 为 `@rpath/librime.1.dylib`。
  - `dist/lib/rime-plugins/` 下有 lua、octagram、predict 三个插件，依赖
    `@rpath/librime.1.dylib`，自身没有 `LC_RPATH`。
- 插件目录由 librime 用 `dladdr` 算出，为 librime 自身所在目录下的
  `rime-plugins/`，与可执行文件无关，所以只要保持 `lib/librime.1.dylib` +
  `lib/rime-plugins/` 的相对布局即可。`RimeTraits.modules` 为 NULL 时默认模块
  已包含 `plugins`，代码不需要改。
- Linux 需要自己从源码构建。librime 的 `deps.mk` 可以把依赖静态编译，CMake 有
  `BUILD_STATIC`、`BUILD_MERGED_PLUGINS` 选项；librime-lua 的 `action-install.sh`
  会拉取树内的 lua 5.4 源码，无需系统 lua。
- opencc 词典（`t2s.json` 等）只在 `user_data_dir/opencc` 和
  `shared_data_dir/opencc` 查找，属于 rime 数据，不随库附带（见 §7）。
  （后续：rime-ice 的繁体切换需要 `s2t.json`，bundled 包改为附带 opencc 数据，
  见 `.plans/active/bundled-opencc/plan.md`。）

## 2. 产物

现有的 `duanyan-<tag>-<target>.tar.gz` 保持不变，每个 target 新增：

```
duanyan-<tag>-<target>-bundled/
  duanyan
  lib/
    librime.so.1                 # Linux：插件已合并进库
    librime.1.dylib              # macOS：实体文件，不是符号链接
    rime-plugins/                # 仅 macOS
      librime-lua.dylib
      librime-octagram.dylib
      librime-predict.dylib
  licenses/                      # librime、boost、glog、leveldb、marisa、opencc、
                                 # yaml-cpp、lua、各插件的许可证
  LICENSE
  README.md
```

两个 macOS target 用同一个 universal dylib。

用户可以解压到任意位置并把 `duanyan` 软链接进 `PATH`，也可以把 `duanyan` 放进
`<prefix>/bin`，把 `lib/*` 放进 `<prefix>/lib`。

## 3. 代码改动：探测包内的 librime

`crates/duanyan/src/paths.rs`：

- `Env` trait 增加 `fn exe_dir(&self) -> Option<PathBuf>`。`RealEnv` 的实现为
  `current_exe()` 经 `canonicalize` 后取 parent，这样软链接也能找到真实目录
  （macOS 的 `current_exe` 不解析软链接）。
- `librime_candidates` 的顺序改为：
  1. 配置文件 `rime.librime_path`
  2. `$DUANYAN_LIBRIME_PATH`
  3. **`<exe_dir>/lib/<name>`、`<exe_dir>/../lib/<name>`（存在才加入）**
  4. 裸文件名，交给系统加载器
  5. 已知目录（Squirrel、Homebrew、nix profile、`/usr/lib` 等）

  包内的库排在系统库之前：用户下载 bundled 包就是想用它；想用系统库时可以写配置
  或设环境变量。nix 包的 wrapper 设了 `DUANYAN_LIBRIME_PATH`，且 nix 包旁边没有
  `lib/librime*`，不受影响。
- `FakeEnv` 增加 `exe_dir`，新增 `librime_bundled` 测试，覆盖两种布局。
- `shared_data_dir` 的探测不变。
- `duanyan info` 已经打印 librime 路径和版本，不需要改。

## 4. macOS：使用官方预编译库

在 workflow 中：

1. 用 `gh release download 1.17.0 -R rime/librime -p 'rime-*-macOS-universal.tar.bz2'`
   下载，并核对 workflow 中固定的 sha256。
2. 用 `cp -L` 把 `librime.1.dylib` 复制为实体文件，连同 `rime-plugins/` 放进
   `lib/`。不修改二进制，保留原有的 ad-hoc 签名。
3. 待验证：插件的 `@rpath/librime.1.dylib` 能否匹配到已加载的 librime。dyld 会先
   按 install name 匹配已加载的镜像，预计可行。如果不行，就用
   `install_name_tool -add_rpath @loader_path/..` 修改插件，再用 `codesign -f -s -`
   重新签名。

## 5. Linux：从源码构建

新增 `scripts/build-librime.sh <outdir>`，版本号都固定在脚本里。CI 和本地
（`docker run ubuntu:22.04`，命令见脚本开头）都能跑：

1. `git clone --depth 1 -b 1.17.0 --recurse-submodules --shallow-submodules`。
2. Boost：下载 1.89 源码并核对 sha256，用 b2 编译静态、PIC 的 regex 库（CMake
   ≥ 3.25 时 Linux 分支的 `find_package` 要求 `regex` 组件）。
3. 依赖：不用 `deps.mk`，直接逐个调用 CMake，统一开启 PIC、构建静态库，并关掉
   可选的系统库（glog 的 unwind/gflags，leveldb 的 snappy/crc32c/tcmalloc），
   避免产物依赖构建机上碰巧装了的东西。跳过 googletest。opencc 生成词典需要
   python3。
4. 插件：lua、octagram、predict，固定到 macOS 包 `version-info.txt` 中的提交
   （`ec52e48`、`dfcc151`、`920bd41`），保证两个平台一致。librime-lua 使用树内的
   lua 5.4 源码。
5. CMake 选项：
   - `BUILD_STATIC=ON`、`BUILD_MERGED_PLUGINS=ON`、`ENABLE_EXTERNAL_PLUGINS=OFF`、
     `BUILD_TEST=OFF`、`CMAKE_BUILD_TYPE=Release`；
   - `CMAKE_SHARED_LINKER_FLAGS="-static-libstdc++ -static-libgcc -Wl,--exclude-libs,ALL"`。

   插件合并进库，Linux 就没有插件目录和 rpath 问题。静态链接 libstdc++，老发行版
   缺少 `GLIBCXX_3.4.30` 也能用。
6. 输出 `librime.so.1`（实体文件，已 `strip`）、`include/`、`licenses/`（lua 的
   许可证从 `lua.h` 末尾提取）、`version-info.txt`，以及仅供测试用的
   `share/opencc`。
7. 自检：
   - `readelf -d` 的 NEEDED 只允许 libc、libm、libpthread、libdl、librt、ld-linux；
   - `objdump -T` 中最高的 GLIBC 版本不超过 2.35。

在 ubuntu-22.04 与 ubuntu-22.04-arm 上构建，glibc 基线与现有二进制一致。

## 6. CI

修改 `.github/workflows/release.yml`：

- `env` 中固定 `LIBRIME_VERSION: 1.17.0`、macOS 包的文件名和 sha256。
- 新 job `librime-linux`（x86_64 / aarch64 矩阵）：
  - 用 `actions/cache` 缓存产物，key 为
    `librime-<arch>-linux-<hashFiles('scripts/build-librime.sh')>`；
  - 首次构建约 15–30 分钟，之后命中缓存；
  - 用 `upload-artifact` 上传。
- tag 上生成的缓存只对该 tag 可见，所以增加 `push` 到 main 的触发（只在
  `scripts/build-librime.sh` 或 workflow 改动时），这时只运行 `librime-linux`，
  在 main 上预热缓存，供之后的 release 使用。
- `build` job `needs: librime-linux`：
  - Linux 条目下载本架构的 artifact；macOS 条目按 §4 下载官方包，另外下载
    x86_64 Linux 的 artifact，取其中的 `licenses/` 和测试用的 opencc 数据
    （两个平台的依赖版本相同）；
  - 现有纯二进制打包不变，新增 bundled 打包；
  - 冒烟测试（x86_64-apple-darwin 在 arm 的 runner 上通过 Rosetta 运行）：
    - 在不设 `DUANYAN_LIBRIME_PATH` 的环境里运行包内的 `duanyan info`，断言输出中
      的路径指向包内 `lib/`，版本为 1.17.0；
    - 用包内的库跑 `cargo test -p rime-dl`（含 layout 与 integration 测试）；
      `DUANYAN_RIME_SHARED_DIR` 由 rime-prelude、rime-essay、rime-luna-pinyin 加
      opencc 数据临时拼成；
  - 上传 `-bundled.tar.gz` 和 `.sha256`。
- `workflow_dispatch` 的 `tag` 改为可选：留空时构建所选分支，产物作为 workflow
  artifact 上传，不上传 release。这样不发版也能验证整个 workflow。

## 7. 限制与文档

- bundled 包只含库，不含 rime 数据。README 不逐条列出数据方面的限制（opencc
  词典、shared_data_dir 等），只给一条路径：下载
  [rime-ice](https://github.com/iDvel/rime-ice) 或
  [rime-frost](https://github.com/gaboolic/rime-frost)，把仓库内容直接放进
  `~/.config/duanyan/rime`（默认 `user_data_dir`）。这两套方案自带词库和 lua
  脚本，用的是简体词库，不依赖 opencc 的 t2s 转换，正好配合包内的插件。
- macOS 用浏览器下载的包带 quarantine 属性，未签名的 dylib 可能被 Gatekeeper
  拦截。README 需说明 `xattr -dr com.apple.quarantine <dir>`；现有纯二进制包也有
  同样的问题。
- README 更新：
  - 安装章节介绍 bundled 包：解压、放进 `PATH`，再按上面的方法放入 rime-ice
    或 rime-frost，然后运行 `duanyan`（首次启动会自动部署）；
  - librime 探测顺序表补上包内路径。
- `AGENTS.md` 补充 `scripts/build-librime.sh` 的用途和本地运行方式。

## 8. 验证

- `nix develop -c cargo test`、`cargo clippy --all-targets`：探测顺序单元测试。
- `nix develop -c scripts/e2e.sh`：终端行为不变，确认没有回归。
- 本地在 `ubuntu:22.04` 容器中跑一次 `scripts/build-librime.sh`，再在宿主机上用
  `DUANYAN_LIBRIME_PATH` 指向产物跑 integration 测试。
- `workflow_dispatch`（`tag` 留空）跑通四个 target。
- 按 README 的步骤（rime-ice 放进 `~/.config/duanyan/rime`）在 Linux 上走一遍
  bundled 包。macOS 需要手动确认一次，重点是插件加载：检查 rime-ice 的 lua 功能
  可用，并且 rime 日志中没有 `error loading plugin`。
