# KKP 下 compat 的 Shift 敲击失效

## 1. 问题

开启 kitty 键盘协议（KKP）后，`[keybinding.compat]` 里的 `Shift_L = "ctrl+l"` /
`Shift_R = "ctrl+r"` 不再切换中英文；直接按物理 Shift 仍然正常。

原因：`KKP_FLAGS` 带 `REPORT_ALL_KEYS_AS_ESCAPE_CODES`，终端会单独报告修饰键。
按 `ctrl+l` 时 rime 收到的顺序是：

1. `Control_L` 按下（`to_rime` 转发的裸修饰键）
2. compat 合成的 `Shift_L` 按下 + 释放
3. `l` 释放、`Control_L` 释放

librime 的 `ascii_composer` 只记录第一个按下的修饰键。`Control_L` 在先，`Shift_L`
不算第一个，于是 `Shift_L` 释放时只清状态、不切换。

已用真实 librime 复现（在 `rime-dl` 集成测试里临时插入上述序列）：先发
`Control_L` 时 `ascii_mode` 不变；不发时正常切换。

不开 KKP 时终端不会单独报 Ctrl，所以没有这个问题。现有单元测试的假引擎不模拟
“第一个修饰键优先”，所以没有发现。

## 2. 决定

`to_rime` 只转发 `LeftShift` / `RightShift` 的单独按下和释放，其它裸修饰键（Ctrl、
Alt、Super、Hyper、Meta）一律不发给 rime。

考虑过的另一种做法：在 `App` 里记录 KKP 下按住的修饰键，命中 compat 时先给它们
补发释放。没有采用，因为它要在 duanyan 里再维护一份 librime 已有的状态，而
history 焦点、busy、失焦时都可能收不到释放事件，两边容易不同步。

代价：schema 里 `ascii_composer/switch_key` 的 `Control_L` / `Control_R` 在 KKP 下
不再生效。影响很小：

- 默认值就是 `noop`，用 Ctrl 敲击切换中英文的人很少。
- 不开 KKP 时这项功能本来就用不了，改后两种模式行为一致。
- Alt、Super 等的单独敲击 `ascii_composer` 本来就不处理。
- Caps Lock 是 `KeyCode::CapsLock`，不属于 `KeyCode::Modifier`，不受影响。
- 需要 Ctrl 敲击时，仍可用 compat 绑定模拟，例如 `Control_L = "f9"`。

## 3. 改动

### 3.1 代码

- `keys::to_rime`：`KeyCode::Modifier` 分支只保留 `LeftShift` / `RightShift`，其它
  返回 `None`。更新函数的文档注释。
- `app.rs` 测试：
  - 假引擎模拟 `ascii_composer` 的“第一个修饰键优先”：记录第一个按下的修饰键，
    只有它是 Shift 且随后释放时才切换。
  - 新测试：`LeftControl` 按下 → `ctrl+l` → `l` 释放 → `LeftControl` 释放，断言
    `ascii_mode` 切换，且引擎没有收到 `Control_L`。改动前这个测试应当失败。
  - `kkp_native_shift_tap` 保持通过。
- `keys.rs` 测试：`to_rime` 对 `LeftControl` 返回 `None`，对 `LeftShift` 照旧。

### 3.2 文档

- AGENTS.md「Terminal pitfalls」加一条，只写冲突本身，不写决定：

  > - Under KKP a chord such as `ctrl+l` arrives as separate events: a bare Ctrl
  >   press, then `l`. librime's `ascii_composer` counts a Shift tap only when no
  >   other modifier went down first, so a Shift tap synthesized for that chord
  >   does nothing if rime has already seen the Ctrl press.

- README 在「多数终端无法单独发送 Shift 键……」一段后加一个默认折叠的
  `<details>`，说明现象、简短原因、我们的做法和替代配置：

  ````markdown
  <details>
  <summary>终端与 rime 的按键处理差异</summary>

  支持 kitty 键盘协议的终端会单独报告 Ctrl、Alt 等修饰键。rime 只在最先按下的修饰键是 Shift 时，才把 Shift 的敲击当作切换中英文。如果把 Ctrl 也发给 rime，按 `ctrl+l` 时 rime 会先看到 Ctrl，模拟的 Shift 就不起作用了。

  因此端砚只把单独的 Shift 发给 rime。代价是 rime 配置里 `ascii_composer/switch_key` 的 `Control_L` / `Control_R` 不会生效（默认都是 `noop`）。如果你习惯用 Ctrl 切换中英文，可以改用 Shift：

  ```yaml
  # ~/.config/duanyan/rime/default.custom.yaml
  patch:
    ascii_composer/switch_key/Shift_L: commit_code
  ```

  或者在端砚里绑一个键来模拟 Ctrl 敲击：

  ```toml
  [keybinding.compat]
  Control_L = "f9"
  ```

  </details>
  ````

## 4. 验证

- `nix develop -c cargo test`、`cargo clippy --all-targets`。
- `scripts/e2e.sh` 照常通过。
- 手动在 kitty（不经 tmux）里验证：`ctrl+l` / `ctrl+r` 切换中英文；直接按左 / 右
  Shift 照常切换。
