# Shell Tab 绑定实现进度

设计见 [`plan.md`](plan.md)。

## 状态

全部完成。

- zsh：加载前 `DUANYAN_TRIGGER=''` 时不绑 Tab；emacs 和 viins 分别记录 `^I` 的原
  widget，`$KEYMAP` 为 `main` 时用 `bindkey -lL main` 解析出实际 keymap。
- fish：加载前 `DUANYAN_TRIGGER=''` 时不绑 Tab；default 和 insert 分别记录原绑定，
  转回时输入函数用 `commandline -f`，其它用 `eval`。
- README「Shell 集成」保持简短，细节放进 `<details>`。

## 与计划的偏差

- 解析 `bind` 输出时用 `string replace -rf`：格式不认识的行直接丢弃，退回
  `commandline -f complete`，不会把整行 `bind ...` 当成命令执行。

## 已验证

- `cargo test`、`cargo clippy --all-targets` 通过。
- `scripts/e2e.sh` 全部通过，新增：
  - zsh：预先给 emacs / viins 绑不同的 widget，加载两次后 Tab 分别转回各自的
    widget（`bindkey -v` 之后走 viins）；`^^` + Tab 照常打开端砚。
  - fish：预先 `bind \t mark end-of-line`，加载两次后 Tab 两条都执行；
    `fish_vi_key_bindings` 之后 insert 模式转回 insert 的原绑定。
  - zsh、fish：加载前设空串时 Tab 未被绑定，手动绑到 `C-x C-d` 的 widget 能插入。
- 手动：fish 3.7.1（nixos-24.05）下转回原绑定、vi 模式均正常，记录的列表正确。
