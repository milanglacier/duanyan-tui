# Shell Tab 绑定实现进度

设计见 [`plan.md`](plan.md)。

## 状态

全部完成。

- zsh：加载前 `DUANYAN_TRIGGER=''` 时不绑 Tab；emacs 和 viins 分别记录 `^I` 的原
  widget，`$KEYMAP` 为 `main` 时用 `bindkey -lL main` 解析出实际 keymap。
- fish：加载前 `DUANYAN_TRIGGER=''` 时不绑 Tab；default 和 insert 分别记录原绑定，
  转回时输入函数用 `commandline -f`，其它用 `eval`；原绑定带 `-m` 时另存目标模式，
  命令跑完后设置 `fish_bind_mode`。`fish_key_bindings` 没有值时先设为
  `fish_default_key_bindings`。
- README「Shell 集成」保持简短，细节放进 `<details>`；autopair.fish 的处理写在
  wiki 的 Fish-autopair-conflict 页，README 只放链接。

## 与计划的偏差

- 解析 `bind` 输出时用 `string replace -rf`：格式不认识的行直接丢弃，退回
  `commandline -f complete`，不会把整行 `bind ...` 当成命令执行。
- fish 原绑定的 `-m` 起初被正则剥掉后丢弃，转回后不切换模式；后来补上（见上）。
- fish 3 的混合绑定（输入函数后跟 shell 命令）不保证顺序：`commandline -f` 只是
  入队，等 `__duanyan_tab` 返回后才执行。fish 3 原生的混合绑定本身就会丢掉 shell
  命令的修改，所以不处理；fish 4 下与原生一致。
- 计划 3.3 认为只有 vi 用户需要先设按键模式，实际上与 autopair.fish 同用时 emacs
  用户也会被盖掉：autopair 在 `conf.d` 里只在 `fish_key_bindings` 有值时才绑 Tab，
  并在它变化时重新绑；fish 到 `config.fish` 之后才给它赋默认值（fish 4 每次启动都
  设为 global，fish 3 首次启动设为 universal），触发 autopair 盖掉端砚。端砚加载时
  先设好同样的默认值，默认按键模式就无需用户处理。vi 模式仍需在加载前
  `set -g fish_key_bindings fish_vi_key_bindings`；在 fish 4 中直接调用
  `fish_vi_key_bindings` 会让 autopair 的 Tab 绑定在端砚加载前丢失。README 里不再
  提按键模式。
- 用户在 `fish_user_key_bindings` 里绑 Tab 时，它在 `config.fish` 之后执行，总会
  盖掉端砚。这种用法少见，不处理也不写进 README。

## 已验证

- `cargo test`、`cargo clippy --all-targets` 通过。
- `scripts/e2e.sh` 全部通过，新增：
  - zsh：预先给 emacs / viins 绑不同的 widget，加载两次后 Tab 分别转回各自的
    widget（`bindkey -v` 之后走 viins）；`^^` + Tab 照常打开端砚。
  - fish：预先 `bind \t mark end-of-line`，加载两次后 Tab 两条都执行；
    `fish_vi_key_bindings` 之后 insert 模式转回 insert 的原绑定
    （`bind -M insert -m default`），并切到 normal 模式。
  - zsh、fish：加载前设空串时 Tab 未被绑定，手动绑到 `C-x C-d` 的 widget 能插入。
- 手动：fish 3.7.1（nixos-24.05）下转回原绑定、vi 模式均正常，记录的列表正确。
- 手动：fish 3.7.1 和 4.9.3 下，`-m` 的模式切换与原生绑定一致；混合绑定
  `backward-char m`、`beginning-of-line m`、`kill-line m` 等在 fish 4 下与原生一致。
- 手动：fish 3.7.1 和 4.9.3 下装 nixpkgs 的 autopair.fish，默认按键模式什么都不设、
  `set -g` 设 vi 后加载、universal 变量已设时，Tab 都是端砚，转回 `_autopair_tab`；
  加载后再设 vi 时会被盖掉，符合 wiki 的说明。
