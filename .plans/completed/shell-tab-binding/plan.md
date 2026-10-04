# Shell 集成：可以不接管 Tab，并与其它插件的 Tab 共存

## 1. 问题

- 没法让端砚不碰 Tab。`DUANYAN_TRIGGER=''` 只是让 Tab 每次都转回原功能，
  Tab 仍然绑在 `__duanyan_tab` 上。
- fish 用 `bind \t __duanyan_tab` 直接覆盖原绑定，并且固定转回
  `commandline -f complete`。autopair.fish（`_autopair_tab`）和 fifc（`_fifc`）这类插件
  装在 `conf.d` 里，比 `config.fish` 先执行，所以端砚一加载就把它们的 Tab 吃掉了。
  端砚先加载时则反过来，端砚被覆盖，没有任何提示。
- zsh 只从默认 keymap 读 `^I` 的原 widget，emacs 和 viins 都转回这一个。
  viins 下 Tab 绑的若是别的 widget，会转错。

## 2. 用法

```sh
# 默认：^^ + Tab 打开端砚，其它情况 Tab 执行原来的功能
eval "$(duanyan init zsh)"

# 不接管 Tab：加载前把触发串设为空串，再自己绑键
DUANYAN_TRIGGER=''
eval "$(duanyan init zsh)"
bindkey '^X^D' duanyan-widget
```

```fish
set -g DUANYAN_TRIGGER ''
duanyan init fish | source
bind \cx\cd duanyan_widget
```

| 变量 | 作用 |
| --- | --- |
| `DUANYAN_TRIGGER` 未设置 | 触发串为 `^^`，接管 Tab |
| `DUANYAN_TRIGGER` 非空 | 按 Tab 时读取，改了立即生效 |
| `DUANYAN_TRIGGER=''`，加载前设置 | 不绑 Tab，只定义 widget |
| `DUANYAN_TRIGGER=''`，加载后设置 | Tab 仍绑着端砚，但每次都转回原功能（与现在相同） |

脚本在当前 shell 里执行，所以加载时就能读到变量，不需要 `export`。不新增变量，也不给
`duanyan init` 加参数。bash 本来就不接管 Tab，不改。

fish 继续用 Tab，不改成 Shift-Tab。fish 自带的 Shift-Tab 是 `complete-and-search`，
`fzf --fish` 也绑了 Shift-Tab，换键并不能避开冲突，还要按 fish 版本区分
`shift-tab` 和 `-k btab` 两种写法。用 Tab 也和 zsh 的 `^^<Tab>` 一致。

## 3. 实现

### 3.1 zsh

- 加载时 `DUANYAN_TRIGGER` 已设置且为空：只定义 `duanyan-widget`，不执行
  `bindkey`，也不记录原 widget。
- 否则 emacs 和 viins 分别用 `bindkey -M <keymap> '^I'` 记下原 widget，存进
  关联数组 `__duanyan_tab_fallback[<keymap>]`。原 widget 已经是 `__duanyan_tab`
  （重复 source）或是 `undefined-key` 时不记录。
- `__duanyan_tab` 转回时用 `$KEYMAP` 查对应的原 widget。查不到时用
  `expand-or-complete`。`$KEYMAP` 为 `main` 时，先解析 main 是 emacs 还是 viins
  的别名，再查表。

### 3.2 fish

- 加载时 `DUANYAN_TRIGGER` 已设置且为空：只定义 `duanyan_widget`，不执行 `bind`。
- 否则 `default` 和 `insert` 两个模式分别记录原绑定：

  ```fish
  for mode in default insert
      set -l line (bind -M $mode \t 2>/dev/null)[-1]
      set -l rest (string replace -r -- '^bind (--preset )?(-M \S+ )?(-m \S+ )?(tab|\\\\t) ' '' $line)
      test "$rest" != __duanyan_tab; and eval "set -g __duanyan_tab_$mode $rest"
  end
  ```

  - `bind` 可能输出两行，fish 自带的那行在前、用户的绑定在后，`[-1]` 取用户的。
  - fish 4 输出的键名是 `tab`，fish 3 是 `\t`。查询时都用 `\t`，两个版本都认。
    已在 fish 4.9 和 3.7.1 上验证。
  - `eval` 把输出里转义过的参数还原成列表，例如
    `bind tab _fifc 'echo second'` 得到 `_fifc` 和 `echo second` 两项。
  - 原绑定已经是 `__duanyan_tab`（重复 source）时不覆盖已记录的值。
- 转回时按 `$fish_bind_mode` 取出对应列表，逐条执行：名字在
  `bind --function-names` 里的用 `commandline -f`，其它用 `eval`。列表为空时执行
  `commandline -f complete`。
- 原绑定带的 `-m`（按完 Tab 后切换按键模式）不保留。Tab 上很少这样用。

上一版有意不解析 `bind` 的输出（见 `shell-integration/progress.md`），理由是格式随
fish 版本变化。现在两种格式都已确认，一个正则就能覆盖。

### 3.3 已知限制（写进 README）

- 只能转回加载端砚时已经存在的绑定，所以端砚要在其它接管 Tab 的插件之后加载：
  - zsh：放在 fzf、fzf-tab、zsh-autocomplete 之后。
  - fish：`conf.d` 里的插件本来就先于 `config.fish` 执行。
- autopair.fish 在每次 `fish_key_bindings` 变化时重新绑 Tab，会盖掉端砚。
  fish 要先设好按键模式（例如 `fish_vi_key_bindings`），再加载端砚。

## 4. 测试

`scripts/e2e.sh`：

- zsh：先 `bindkey '^I'` 到一个自定义 widget（向 `LBUFFER` 追加标记），再加载
  端砚。没有触发串时按 Tab 出现标记；`^^` + Tab 照常打开端砚。
- zsh：`bindkey -v` 后在 viins 下，没有触发串时按 Tab 转回 viins 的原 widget。
- fish：先 `bind \t` 到一个带两条命令的绑定（一个函数加一个
  `commandline -f` 能执行的名字），再加载端砚，检查两条都执行了。
- zsh、fish：加载前把 `DUANYAN_TRIGGER` 设为空串，`bindkey '^I'` / `bind \t` 的结果
  与加载前相同；`duanyan-widget` / `duanyan_widget` 绑到 `C-x C-d` 后能插入文字。
- 原有的 `integration_checks` 保持通过。

手动：fish 3.x（例如 `nix run github:NixOS/nixpkgs/nixos-24.05#fish`）里跑一遍
「转回原绑定」，devShell 里只有 fish 4。

## 5. 文档

README 的「Shell 集成」只保留一句话说明、zsh / fish 的加载命令和 bash 的绑定示例。
其余内容放进 `<details><summary>自定义</summary>` 折叠：

- 修改触发串：`DUANYAN_TRIGGER`。
- 不接管 Tab：加载前设为空串，附 zsh / fish 手动绑键示例。
- 加载顺序：放在其它接管 Tab 的插件之后；fish 先设好按键模式再加载。
- bash 不接管 Tab 的说明。

脚本开头的注释同步更新：说明空触发串表示不绑 Tab，以及 Tab 会转回原来的绑定。
