#!/usr/bin/env bash
# End-to-end tests: runs the real duanyan binary in tmux panes (a real pty
# and terminal emulator), sends keys, and asserts on the rendered screen,
# the tmux clipboard (OSC 52), --stdout output and edited files.
#
# Run inside `nix develop` (provides tmux, librime and rime-data):
#   nix develop -c scripts/e2e.sh
#
# Uses a private tmux server and temporary XDG dirs; the user's tmux and
# duanyan configuration are never touched. Set KEEP=1 to keep the temp dir.

# No `set -e`: a failed check is counted and the run continues.
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
: "${DUANYAN_LIBRIME_PATH:?run inside nix develop}"
: "${DUANYAN_RIME_SHARED_DIR:?run inside nix develop}"
command -v tmux >/dev/null || { echo "tmux not found" >&2; exit 2; }

cargo build --quiet --manifest-path "$root/Cargo.toml" -p duanyan || exit 2
bin="$root/target/debug/duanyan"

tmp=$(mktemp -d "${TMPDIR:-/tmp}/duanyan-e2e.XXXXXX")
sock="duanyan-e2e-$$"
cleanup() {
    tmux -L "$sock" kill-server 2>/dev/null || true
    if [[ -z "${KEEP:-}" ]]; then rm -rf "$tmp"; else echo "kept $tmp"; fi
}
trap cleanup EXIT

# Isolated config; one simplified schema keeps the first deploy fast and the
# expected characters deterministic.
mkdir -p "$tmp/config/duanyan/rime" "$tmp/state"
printf 'patch:\n  schema_list:\n    - schema: luna_pinyin_simp\n' \
    >"$tmp/config/duanyan/rime/default.custom.yaml"

# The UI language follows the locale; pin it so that the developer's locale
# does not change the expected text. DUANYAN_E2E_LOCALE selects another one.
cat >"$tmp/duanyan" <<EOF
#!/usr/bin/env bash
export XDG_CONFIG_HOME="$tmp/config" XDG_STATE_HOME="$tmp/state"
export DUANYAN_LIBRIME_PATH="$DUANYAN_LIBRIME_PATH"
export DUANYAN_RIME_SHARED_DIR="$DUANYAN_RIME_SHARED_DIR"
# Through env: bash would warn when it cannot load the locale itself.
exec env -u LANGUAGE -u LC_ALL LC_MESSAGES="\${DUANYAN_E2E_LOCALE:-zh_CN.UTF-8}" "$bin" "\$@"
EOF
chmod +x "$tmp/duanyan"

t() { tmux -L "$sock" -f /dev/null "$@"; }

failures=0
screen() { t capture-pane -p -t "$1"; }

# wait_for PANE TEXT [SECONDS]: polls the screen until TEXT appears.
wait_for() {
    local pane=$1 text=$2 deadline=$((SECONDS + ${3:-20}))
    while ((SECONDS < deadline)); do
        [[ "$(screen "$pane")" == *"$text"* ]] && return 0
        sleep 0.1
    done
    echo "FAIL: [$pane] timed out waiting for: $text" >&2
    screen "$pane" | sed 's/^/    | /' >&2
    failures=$((failures + 1))
    return 1
}

check() {
    local what=$1 got=$2 want=$3
    if [[ "$got" == "$want" ]]; then
        echo "ok: $what"
    else
        echo "FAIL: $what: got [$got], want [$want]" >&2
        failures=$((failures + 1))
    fi
}

step() { echo "== $*"; }

# mouse PANE down|drag|up X Y: sends a left-button SGR mouse report at the
# 1-based cell X, Y. tmux passes it through to the program unchanged.
mouse() {
    local b=0 end=M
    case $2 in
    drag) b=32 ;;
    up) end=m ;;
    esac
    t send-keys -t "$1" -l $'\e'"[<$b;$3;$4$end"
}

# A pane running a plain shell, for commands whose exit status we check.
shell_pane() {
    t new-window -d -n "$1" "env -i PATH=\"$PATH\" TERM=screen bash --norc --noprofile"
    sleep 0.3
}

step "fullscreen: first-run deploy, compose, multi-line submit, OSC 52"
t new-session -d -s e2e -n full -x 100 -y 30 "$tmp/duanyan; echo EXIT=\$?; sleep 600"
# Let applications set the tmux clipboard through OSC 52.
t set -s set-clipboard on
wait_for e2e:full "朙月拼音·简化字" 60 && echo "ok: first deploy finished"
t send-keys -t e2e:full -l "nihao"
wait_for e2e:full "你好" && wait_for e2e:full "nihao" && echo "ok: candidates and raw input"
t send-keys -t e2e:full -l " "
t send-keys -t e2e:full C-j
t send-keys -t e2e:full -l "shijie "
t send-keys -t e2e:full Enter
wait_for e2e:full "你好↵世界" && echo "ok: history shows the submitted text"
wait_for e2e:full "已复制到剪贴板" && echo "ok: copied marker"
check "OSC 52 clipboard content" "$(t show-buffer)" $'你好\n世界'

step "fullscreen: compat ctrl+l / ctrl+r toggle ascii mode"
t send-keys -t e2e:full C-l
wait_for e2e:full " 西 " && echo "ok: status bar shows ascii mode"
t send-keys -t e2e:full -l "abc"
wait_for e2e:full "│ abc" && echo "ok: ascii text goes straight to the buffer"

step "fullscreen: word and buffer motion"
t send-keys -t e2e:full C-u
t send-keys -t e2e:full -l "hello world foo"
t send-keys -t e2e:full M-b
t send-keys -t e2e:full -l "1"
t send-keys -t e2e:full M-b M-b
t send-keys -t e2e:full -l "2"
t send-keys -t e2e:full M-f
t send-keys -t e2e:full -l "3"
t send-keys -t e2e:full "M-<"
t send-keys -t e2e:full -l "4"
t send-keys -t e2e:full "M->"
t send-keys -t e2e:full -l "5"
wait_for e2e:full "│ 4hello 2world3 1foo5 " && echo "ok: alt+b, alt+f, alt+<, alt+>"

step "fullscreen: undo and redo"
t send-keys -t e2e:full C-u
t send-keys -t e2e:full -l "one two, three"
wait_for e2e:full "│ one two, three "
# Typing up to the punctuation is one step, the rest another.
t send-keys -t e2e:full C-z
wait_for e2e:full "│ one two,  " && echo "ok: ctrl+z undoes back to the punctuation"
t send-keys -t e2e:full C-_ C-z
wait_for e2e:full "│ 4hello 2world3 1foo5 " && echo "ok: ctrl+_ undoes, then the kill is undone"
t send-keys -t e2e:full C-y M-_
wait_for e2e:full "│ one two,  " && echo "ok: ctrl+y and alt+_ redo"
t send-keys -t e2e:full C-y
wait_for e2e:full "│ one two, three " && echo "ok: redo restores the last step"

step "fullscreen: mouse click, drag copy, backspace deletes the selection"
t send-keys -t e2e:full C-u
t send-keys -t e2e:full -l "hello world"
wait_for e2e:full "│ hello world "
row=$(screen e2e:full | grep -n "│ hello world " | cut -d: -f1)
# The text starts at column 4 (1-based): drag from "w" past the row end.
mouse e2e:full down 10 "$row"
mouse e2e:full drag 20 "$row"
mouse e2e:full up 20 "$row"
wait_for e2e:full "已复制选中文字" && check "drag copies the selection" "$(t show-buffer)" "world"
t send-keys -t e2e:full BSpace
wait_for e2e:full "│ hello  " && echo "ok: backspace deleted the selection"
mouse e2e:full down 4 "$row"
mouse e2e:full up 4 "$row"
t send-keys -t e2e:full -l "X"
wait_for e2e:full "│ Xhello " && echo "ok: click moved the cursor"
t send-keys -t e2e:full C-e C-u
t send-keys -t e2e:full C-r
wait_for e2e:full " 中 " && echo "ok: back to chinese mode"

step "fullscreen: history focus, recall"
t send-keys -t e2e:full C-u
t send-keys -t e2e:full Tab
wait_for e2e:full "y  复制" && echo "ok: history focus hints"
t send-keys -t e2e:full Enter
wait_for e2e:full "Tab  历史" && echo "ok: recall returns to input"

step "secondary instance degrades"
t new-window -d -n second "$tmp/duanyan; sleep 600"
wait_for e2e:second "已有端砚在运行，不学习新词" && echo "ok: secondary warning"
t kill-window -t e2e:second

step "traditional Chinese UI from the locale, simplified from the config"
t new-window -d -n trad "DUANYAN_E2E_LOCALE=zh_TW.UTF-8 $tmp/duanyan; sleep 600"
wait_for e2e:trad "端硯-tui" && echo "ok: traditional header"
wait_for e2e:trad "已有端硯在執行，不學習新詞" && wait_for e2e:trad "Tab  歷史" &&
    echo "ok: traditional status bar"
t send-keys -t e2e:trad F1
wait_for e2e:trad "說明 · j/k 捲動" && wait_for e2e:trad "全域（不經過 rime）" && echo "ok: traditional help"
t send-keys -t e2e:trad Escape
t send-keys -t e2e:trad -l "nihao "
t send-keys -t e2e:trad Enter
wait_for e2e:trad "已複製到剪貼簿" && echo "ok: traditional copied marker"
t kill-window -t e2e:trad
printf '[tui]\nlanguage = "simplified"\n' >"$tmp/simplified.toml"
t new-window -d -n simp "DUANYAN_E2E_LOCALE=zh_TW.UTF-8 $tmp/duanyan --config $tmp/simplified.toml; sleep 600"
wait_for e2e:simp "已有端砚在运行，不学习新词" && echo "ok: the config overrides the locale"
t kill-window -t e2e:simp

step "kitty keyboard protocol: compat ctrl+l with a bare Ctrl press, bare Shift tap"
# tmux does not forward bare modifier keys, so write the CSI u sequences a
# KKP terminal sends straight into the pane.
printf '[tui]\nkitty_keyboard = "on"\n' >"$tmp/kkp.toml"
t new-window -d -n kkp "$tmp/duanyan --config $tmp/kkp.toml; sleep 600"
wait_for e2e:kkp "朙月拼音·简化字" && wait_for e2e:kkp " 中 "
t send-keys -t e2e:kkp -l $'\e[57442;5u' $'\e[108;5u' $'\e[108;5:3u' $'\e[57442;1:3u'
wait_for e2e:kkp " 西 " && echo "ok: ctrl+l toggles ascii mode while Ctrl is reported"
t send-keys -t e2e:kkp -l $'\e[57441;2u' $'\e[57441;1:3u'
wait_for e2e:kkp " 中 " && echo "ok: bare Shift tap toggles back"
t kill-window -t e2e:kkp

check "--help follows the locale" "$(DUANYAN_E2E_LOCALE=zh_TW.UTF-8 "$tmp/duanyan" --help | head -1)" \
    "端硯：基於 rime 的終端機中文輸入草稿板"

step "fullscreen: ctrl+c quits with 0"
t send-keys -t e2e:full C-c
wait_for e2e:full "EXIT=0" && echo "ok: exit status 0"

step "--stdout: submit prints the text"
shell_pane sh1
t send-keys -t e2e:sh1 -l "out=\$($tmp/duanyan --stdout); printf 'code=%s out=[%s]\n' \$? \"\$out\""
t send-keys -t e2e:sh1 Enter
wait_for e2e:sh1 "输出"
t send-keys -t e2e:sh1 -l "nihao "
t send-keys -t e2e:sh1 C-j
t send-keys -t e2e:sh1 -l "shijie "
t send-keys -t e2e:sh1 Enter
wait_for e2e:sh1 "code=0 out=[你好" && wait_for e2e:sh1 "世界]" && echo "ok: stdout output and status"

step "--stdout: undo and redo inline"
shell_pane sh4
t send-keys -t e2e:sh4 -l "out=\$($tmp/duanyan --stdout); printf 'code=%s out=[%s]\n' \$? \"\$out\""
t send-keys -t e2e:sh4 Enter
wait_for e2e:sh4 "输出"
t send-keys -t e2e:sh4 -l "nihao shijie "
t send-keys -t e2e:sh4 C-z C-z M-_ Enter
wait_for e2e:sh4 "code=0 out=[你好]" && echo "ok: each commit is one undo step"

step "--stdout: esc cancels with 1"
shell_pane sh2
t send-keys -t e2e:sh2 -l "out=\$($tmp/duanyan --stdout); printf 'code=%s out=[%s]\n' \$? \"\$out\""
t send-keys -t e2e:sh2 Enter
wait_for e2e:sh2 "输出"
t send-keys -t e2e:sh2 -l "zhong"
wait_for e2e:sh2 "zhong"
t send-keys -t e2e:sh2 Escape # clears the composition
sleep 0.3
t send-keys -t e2e:sh2 Escape # cancels
wait_for e2e:sh2 "code=1 out=[]" && echo "ok: cancel status"

step "--stdout: near the bottom, the output above stays visible"
shell_pane sh3
t send-keys -t e2e:sh3 -l "clear; seq 101 126; out=\$($tmp/duanyan --stdout); echo \"[\$out]\""
t send-keys -t e2e:sh3 Enter
wait_for e2e:sh3 "输出"
t send-keys -t e2e:sh3 Escape
# Clearing the inline area on exit would erase 126 if the UI covered it.
wait_for e2e:sh3 $'126\n[]' && echo "ok: output above the inline UI kept"

# Shell integration panes: a clean shell with $tmp (the duanyan wrapper) on
# PATH and a fixed prompt.
integration_pane() {
    local name=$1 cmd=$2
    t new-window -d -n "$name" \
        "env -i PATH=\"$tmp:$PATH\" TERM=screen HOME=\"$tmp/home\" XDG_CONFIG_HOME=\"$tmp/home/.config\" $cmd"
}
mkdir -p "$tmp/home"

# integration_checks PANE PROMPT: ^^ + Tab inserts, Esc keeps the line, Tab
# without the trigger still completes.
integration_checks() {
    local pane=$1 prompt=$2
    t send-keys -t "$pane" -l "echo ^^"
    t send-keys -t "$pane" Tab
    wait_for "$pane" "输出" || return
    [[ "$(screen "$pane")" == *"$prompt echo ^^"* ]] && echo "ok: [$pane] prompt line kept while open" ||
        { echo "FAIL: [$pane] prompt line drawn over" >&2; failures=$((failures + 1)); }
    t send-keys -t "$pane" -l "nihao "
    t send-keys -t "$pane" Enter
    wait_for "$pane" "$prompt echo 你好" && echo "ok: [$pane] trigger replaced by the text"
    t send-keys -t "$pane" -l "!"
    wait_for "$pane" "$prompt echo 你好!" && echo "ok: [$pane] cursor after the inserted text"
    t send-keys -t "$pane" Enter
    wait_for "$pane" $'\n你好!' && echo "ok: [$pane] command runs"

    t send-keys -t "$pane" -l "echo ^^"
    t send-keys -t "$pane" Tab
    wait_for "$pane" "输出" || return
    t send-keys -t "$pane" Escape
    sleep 0.5
    t send-keys -t "$pane" -l "x"
    wait_for "$pane" "$prompt echo ^^x" && echo "ok: [$pane] cancel keeps the line"
    t send-keys -t "$pane" C-u

    t send-keys -t "$pane" -l "echo duanyan-e2e-mark; seq 1 3; ech"
    t send-keys -t "$pane" Tab
    wait_for "$pane" "duanyan-e2e-mark; seq 1 3; echo" && echo "ok: [$pane] plain Tab still completes"
    t send-keys -t "$pane" C-u
}

step "shell integration: zsh"
integration_pane zsh "zsh -f"
wait_for e2e:zsh "%"
# /etc/zshenv is read even with -f and may reset PATH (NixOS does).
t send-keys -t e2e:zsh -l "PATH=\"$tmp:\$PATH\"; PS1='zsh> '; eval \"\$(duanyan init zsh)\"; clear"
t send-keys -t e2e:zsh Enter
wait_for e2e:zsh "zsh> " && integration_checks e2e:zsh "zsh>"

step "shell integration: fish"
integration_pane fish "fish --no-config"
wait_for e2e:fish ">"
t send-keys -t e2e:fish -l "function fish_prompt; echo -n 'fish> '; end; duanyan init fish | source; clear"
t send-keys -t e2e:fish Enter
wait_for e2e:fish "fish> " && integration_checks e2e:fish "fish>"

# Tab without the trigger runs the binding it had before the script loaded,
# per keymap, also after loading the script twice.
step "shell integration: zsh keeps the previous Tab widgets"
integration_pane zsh-chain "zsh -f"
wait_for e2e:zsh-chain "%"
t send-keys -t e2e:zsh-chain -l "PATH=\"$tmp:\$PATH\"; PS1='zsh> '; emark() { LBUFFER+=-emark }; vmark() { LBUFFER+=-vmark }; zle -N emark; zle -N vmark; bindkey -M emacs '^I' emark; bindkey -M viins '^I' vmark; eval \"\$(duanyan init zsh)\"; eval \"\$(duanyan init zsh)\"; clear"
t send-keys -t e2e:zsh-chain Enter
wait_for e2e:zsh-chain "zsh> "
t send-keys -t e2e:zsh-chain -l "echo a"
t send-keys -t e2e:zsh-chain Tab
wait_for e2e:zsh-chain "zsh> echo a-emark" && echo "ok: [zsh] Tab runs the previous emacs widget"
t send-keys -t e2e:zsh-chain C-u
t send-keys -t e2e:zsh-chain -l "echo ^^"
t send-keys -t e2e:zsh-chain Tab
wait_for e2e:zsh-chain "输出" && echo "ok: [zsh] trigger still opens duanyan"
t send-keys -t e2e:zsh-chain Escape
wait_for e2e:zsh-chain "zsh> echo ^^"
t send-keys -t e2e:zsh-chain C-u
t send-keys -t e2e:zsh-chain -l "bindkey -v"
t send-keys -t e2e:zsh-chain Enter
t send-keys -t e2e:zsh-chain -l "echo b"
t send-keys -t e2e:zsh-chain Tab
wait_for e2e:zsh-chain "zsh> echo b-vmark" && echo "ok: [zsh] Tab runs the previous viins widget"

step "shell integration: fish keeps the previous Tab binding"
integration_pane fish-chain "fish --no-config"
wait_for e2e:fish-chain ">"
t send-keys -t e2e:fish-chain -l "function fish_prompt; echo -n 'fish> '; end; function mark; commandline -i -- -mark; end; function imark; commandline -i -- -imark; end; bind \\t mark end-of-line; bind -M insert -m default \\t imark; duanyan init fish | source; duanyan init fish | source; clear"
t send-keys -t e2e:fish-chain Enter
wait_for e2e:fish-chain "fish> "
t send-keys -t e2e:fish-chain -l "echo ab"
t send-keys -t e2e:fish-chain Left Tab
t send-keys -t e2e:fish-chain -l "!"
wait_for e2e:fish-chain "fish> echo a-markb!" && echo "ok: [fish] Tab runs every command of the previous binding"
t send-keys -t e2e:fish-chain C-u
t send-keys -t e2e:fish-chain -l "echo ^^"
t send-keys -t e2e:fish-chain Tab
wait_for e2e:fish-chain "输出" && echo "ok: [fish] trigger still opens duanyan"
t send-keys -t e2e:fish-chain Escape
wait_for e2e:fish-chain "fish> echo ^^"
t send-keys -t e2e:fish-chain C-u
t send-keys -t e2e:fish-chain -l "fish_vi_key_bindings"
t send-keys -t e2e:fish-chain Enter
t send-keys -t e2e:fish-chain -l "echo cd"
t send-keys -t e2e:fish-chain Left Tab
# In normal mode, x deletes the d and A appends; in insert mode both would be
# inserted.
t send-keys -t e2e:fish-chain -l "xA!"
wait_for e2e:fish-chain "fish> echo c-imark!" && echo "ok: [fish] Tab runs the previous insert-mode binding and its mode switch"

# An empty DUANYAN_TRIGGER set before loading leaves Tab alone; the widget
# bound to another key still inserts text.
step "shell integration: empty trigger leaves Tab alone"
integration_pane zsh-notab "zsh -f"
wait_for e2e:zsh-notab "%"
t send-keys -t e2e:zsh-notab -l "PATH=\"$tmp:\$PATH\"; PS1='zsh> '; DUANYAN_TRIGGER=''; eval \"\$(duanyan init zsh)\"; bindkey '^X^D' duanyan-widget; clear"
t send-keys -t e2e:zsh-notab Enter
wait_for e2e:zsh-notab "zsh> "
t send-keys -t e2e:zsh-notab -l "bindkey -M emacs '^I' | grep -q __duanyan_tab || echo tab-un''touched"
t send-keys -t e2e:zsh-notab Enter
wait_for e2e:zsh-notab $'\ntab-untouched' && echo "ok: [zsh] Tab not bound"
t send-keys -t e2e:zsh-notab -l "echo "
t send-keys -t e2e:zsh-notab C-x C-d
wait_for e2e:zsh-notab "输出"
t send-keys -t e2e:zsh-notab -l "nihao "
t send-keys -t e2e:zsh-notab Enter
wait_for e2e:zsh-notab "zsh> echo 你好" && echo "ok: [zsh] duanyan-widget bound to another key"

integration_pane fish-notab "fish --no-config"
wait_for e2e:fish-notab ">"
t send-keys -t e2e:fish-notab -l "function fish_prompt; echo -n 'fish> '; end; set -g DUANYAN_TRIGGER ''; duanyan init fish | source; bind \\cx\\cd duanyan_widget; clear"
t send-keys -t e2e:fish-notab Enter
wait_for e2e:fish-notab "fish> "
t send-keys -t e2e:fish-notab -l "bind \\t | string match -q '*__duanyan_tab*'; or echo tab-un''touched"
t send-keys -t e2e:fish-notab Enter
wait_for e2e:fish-notab $'\ntab-untouched' && echo "ok: [fish] Tab not bound"
t send-keys -t e2e:fish-notab -l "echo "
t send-keys -t e2e:fish-notab C-x C-d
wait_for e2e:fish-notab "输出"
t send-keys -t e2e:fish-notab -l "nihao "
t send-keys -t e2e:fish-notab Enter
wait_for e2e:fish-notab "fish> echo 你好" && echo "ok: [fish] duanyan_widget bound to another key"

step "shell integration: bash widget"
integration_pane bash "bash --norc --noprofile"
wait_for e2e:bash "$"
t send-keys -t e2e:bash -l "PS1='bash> '; eval \"\$(duanyan init bash)\"; bind -x '\"\\C-x\\C-d\": __duanyan_widget'; clear"
t send-keys -t e2e:bash Enter
wait_for e2e:bash "bash> "
t send-keys -t e2e:bash -l "echo 中文ab"
t send-keys -t e2e:bash Left Left C-x C-d
wait_for e2e:bash "输出"
t send-keys -t e2e:bash -l "nihao "
t send-keys -t e2e:bash Enter
wait_for e2e:bash "bash> echo 中文你好ab" && echo "ok: [bash] text inserted at the cursor"
t send-keys -t e2e:bash -l "!"
wait_for e2e:bash "bash> echo 中文你好!ab" && echo "ok: [bash] cursor after the inserted text"

# edit_pane NAME FILE: opens FILE in duanyan from a shell pane that prints
# the exit status afterwards; waits for the edit-mode status hints.
edit_pane() {
    shell_pane "$1"
    t send-keys -t "e2e:$1" -l "$tmp/duanyan $2; echo code=\$?"
    t send-keys -t "e2e:$1" Enter
    wait_for "e2e:$1" "Enter  保存"
}

step "edit FILE: tabs shown, typing and Enter save with 0"
printf 'hello\n#\tcomment\n' >"$tmp/edit1.txt"
edit_pane ed1 "$tmp/edit1.txt"
wait_for e2e:ed1 "#       comment" && echo "ok: tab expanded to the next tab stop"
t send-keys -t e2e:ed1 -l "nihao "
wait_for e2e:ed1 "你好hello"
t send-keys -t e2e:ed1 Enter
wait_for e2e:ed1 "code=0" && echo "ok: save status"
check "saved file" "$(cat "$tmp/edit1.txt"; echo .)" $'你好hello\n#\tcomment\n.'

# row_of PANE TEXT: the 0-based screen row where TEXT first appears.
row_of() { screen "$1" | grep -n -F -m1 -- "$2" | cut -d: -f1 | awk '{ print $1 - 1 }'; }

step "edit FILE: candidates float next to the cursor"
seq -f 'line %g' 1 40 >"$tmp/long.txt"
edit_pane ed5 "$tmp/long.txt"
t send-keys -t e2e:ed5 Down End
t send-keys -t e2e:ed5 -l "nihao"
wait_for e2e:ed5 "1 你好" &&
    check "candidates right below the cursor" \
        "$(($(row_of e2e:ed5 "1 你好") - $(row_of e2e:ed5 "line 2ni hao")))" 1
t send-keys -t e2e:ed5 -l " "
wait_for e2e:ed5 "line 2你好"
t send-keys -t e2e:ed5 "M->"
t send-keys -t e2e:ed5 -l "nihao"
wait_for e2e:ed5 "1 你好" &&
    check "candidates right above the cursor at the bottom" \
        "$(($(row_of e2e:ed5 "ni hao") - $(row_of e2e:ed5 "1 你好")))" 1
t send-keys -t e2e:ed5 -l " "
t send-keys -t e2e:ed5 Enter
wait_for e2e:ed5 "code=0"
check "saved long file" "$(sed -n '2p;$p' "$tmp/long.txt")" $'line 2你好\n你好'

step "edit FILE: esc on a modified file asks again, then exits 1"
edit_pane ed2 "$tmp/edit1.txt"
t send-keys -t e2e:ed2 -l "nihao "
wait_for e2e:ed2 "你好你好hello"
t send-keys -t e2e:ed2 Escape
wait_for e2e:ed2 "再按一次放弃修改" && echo "ok: discard prompt"
t send-keys -t e2e:ed2 Escape
wait_for e2e:ed2 "code=1" && echo "ok: discard status"
check "discarded file unchanged" "$(cat "$tmp/edit1.txt"; echo .)" $'你好hello\n#\tcomment\n.'

step "edit FILE: a missing file is created on save"
edit_pane ed3 "$tmp/new.txt"
t send-keys -t e2e:ed3 -l "nihao "
wait_for e2e:ed3 "你好"
t send-keys -t e2e:ed3 Enter
wait_for e2e:ed3 "code=0"
check "created file" "$(cat "$tmp/new.txt"; echo .)" "你好."

step "edit FILE: a failed save keeps the editor open"
printf 'ro\n' >"$tmp/ro.txt"
chmod 444 "$tmp/ro.txt"
edit_pane ed4 "$tmp/ro.txt"
t send-keys -t e2e:ed4 Enter
wait_for e2e:ed4 "保存失败" && echo "ok: save error shown"
t send-keys -t e2e:ed4 Escape
wait_for e2e:ed4 "code=1" && echo "ok: unmodified file cancels at once"

step "edit FILE: invalid UTF-8 fails before opening the UI"
printf 'a\xff\n' >"$tmp/bad.txt"
"$tmp/duanyan" "$tmp/bad.txt" </dev/null >/dev/null 2>"$tmp/bad.err"
check "invalid UTF-8 status" "$?" 2
[[ "$(cat "$tmp/bad.err")" == *"not valid UTF-8"* ]] && echo "ok: invalid UTF-8 message" ||
    { echo "FAIL: invalid UTF-8 message: $(cat "$tmp/bad.err")" >&2; failures=$((failures + 1)); }

step "edit FILE: git commit with GIT_EDITOR"
git init -q "$tmp/repo"
git -C "$tmp/repo" config user.name e2e
git -C "$tmp/repo" config user.email e2e@example.com
# The user's global config may sign commits, which would wait for gpg.
git -C "$tmp/repo" config commit.gpgsign false
git -C "$tmp/repo" commit -q --allow-empty -m first
gitcmd="cd $tmp/repo && GIT_EDITOR=$tmp/duanyan git commit -q --allow-empty; echo code=\$? count=\$(git rev-list --count HEAD) subject=\$(git log -1 --format=%s)"
shell_pane git1
t send-keys -t e2e:git1 -l "$gitcmd"
t send-keys -t e2e:git1 Enter
wait_for e2e:git1 "Enter  保存"
t send-keys -t e2e:git1 -l "nihao "
t send-keys -t e2e:git1 Enter
wait_for e2e:git1 "code=0 count=2 subject=你好" && echo "ok: git commit message from duanyan"
shell_pane git2
t send-keys -t e2e:git2 -l "$gitcmd"
t send-keys -t e2e:git2 Enter
wait_for e2e:git2 "Enter  保存"
t send-keys -t e2e:git2 Escape
wait_for e2e:git2 "code=1 count=2" && echo "ok: cancel aborts the commit"

step "bundled: opencc data next to the binary is the last-resort shared data"
# The `-bundled` archive layout, where share/rime-data holds only opencc data.
# The nix librime would find a stock config such as s2t.json in its own opencc
# package, so the schema uses a config that only the bundled dir has.
system_rime=
for d in /usr/share/rime-data /usr/local/share/rime-data /opt/homebrew/share/rime-data \
    "/Library/Input Methods/Squirrel.app/Contents/SharedSupport"; do
    [[ -e $d ]] && system_rime=$d
done
if [[ -n $system_rime ]]; then
    echo "skip: $system_rime exists and takes precedence over the bundled data"
else
    bundle="$tmp/bundle"
    buser="$tmp/bundled/config/duanyan/rime"
    mkdir -p "$bundle/share/rime-data/opencc" "$buser" "$tmp/bundled/state" "$tmp/empty"
    cp "$bin" "$bundle/duanyan"
    # Launched through a symlink, like a copy linked into PATH.
    ln -s "$bundle/duanyan" "$tmp/duanyan-bundled"
    cat >"$bundle/share/rime-data/opencc/e2e_marker.json" <<'EOF'
{
  "name": "duanyan e2e marker",
  "segmentation": {"type": "mmseg", "dict": {"type": "text", "file": "e2e_marker.txt"}},
  "conversion_chain": [{"dict": {"type": "text", "file": "e2e_marker.txt"}}]
}
EOF
    printf '你好\t包内标记\n' >"$bundle/share/rime-data/opencc/e2e_marker.txt"
    # The user dir has everything else, as with rime-ice: the prelude and a
    # one-word schema that converts through the marker config.
    for f in default key_bindings punctuation symbols; do
        cp "$DUANYAN_RIME_SHARED_DIR/$f.yaml" "$buser/"
    done
    printf 'patch:\n  schema_list:\n    - schema: e2e_bundled\n' >"$buser/default.custom.yaml"
    cat >"$buser/e2e_bundled.schema.yaml" <<'EOF'
schema:
  schema_id: e2e_bundled
  name: 端砚测试
  version: "1"
switches:
  - name: marker
    reset: 1
    states: [关, 开]
engine:
  processors: [speller, selector, navigator, express_editor]
  segmentors: [abc_segmentor, fallback_segmentor]
  translators: [table_translator]
  filters: [simplifier, uniquifier]
speller:
  alphabet: abcdefghijklmnopqrstuvwxyz
translator:
  dictionary: e2e_bundled
simplifier:
  option_name: marker
  opencc_config: e2e_marker.json
EOF
    # librime fails to compile a table with a single entry.
    printf -- '---\nname: e2e_bundled\nversion: "1"\n...\n你好\tnihao\n世界\tshijie\n' \
        >"$buser/e2e_bundled.dict.yaml"
    benv="env -u DUANYAN_RIME_SHARED_DIR XDG_DATA_DIRS=$tmp/empty XDG_CONFIG_HOME=$tmp/bundled/config XDG_STATE_HOME=$tmp/bundled/state"

    info=$($benv "$tmp/duanyan-bundled" info | grep '^shared_data_dir ')
    check "info shows the bundled shared data dir" "$info" \
        "shared_data_dir  $(cd "$bundle" && pwd -P)/share/rime-data (bundled opencc data)"
    t new-window -d -n bundled "$benv $tmp/duanyan-bundled; sleep 600"
    wait_for e2e:bundled "端砚测试" 60
    t send-keys -t e2e:bundled -l "nihao"
    wait_for e2e:bundled "包内标记" && echo "ok: librime converts with the bundled opencc config"
    check "bundled stderr.log is empty" "$(cat "$tmp/bundled/state/duanyan/log/stderr.log" 2>/dev/null)" ""
fi

step "bundled: schemas next to the binary win over system rime data"
# The `-bundled-frost` archive layout, with a small schema in place of
# rime-frost. A system rime data dir, which would bring its own default.yaml,
# is simulated through XDG_DATA_DIRS. The user dir starts empty.
sbundle="$tmp/bundle-schemas"
sdata="$sbundle/share/rime-data"
mkdir -p "$sdata" "$tmp/schemas/state" "$tmp/system"
ln -s "$DUANYAN_RIME_SHARED_DIR" "$tmp/system/rime-data"
cp "$bin" "$sbundle/duanyan"
for f in key_bindings punctuation symbols; do
    cp "$DUANYAN_RIME_SHARED_DIR/$f.yaml" "$sdata/"
done
printf 'config_version: "0.1"\nschema_list:\n  - schema: e2e_schemas\n' >"$sdata/default.yaml"
cat >"$sdata/e2e_schemas.schema.yaml" <<'EOF'
schema:
  schema_id: e2e_schemas
  name: 包内方案
  version: "1"
engine:
  processors: [speller, selector, navigator, express_editor]
  segmentors: [abc_segmentor, fallback_segmentor]
  translators: [table_translator]
speller:
  alphabet: abcdefghijklmnopqrstuvwxyz
translator:
  dictionary: e2e_schemas
EOF
printf -- '---\nname: e2e_schemas\nversion: "1"\n...\n包内词条\tnihao\n世界\tshijie\n' \
    >"$sdata/e2e_schemas.dict.yaml"
senv="env -u DUANYAN_RIME_SHARED_DIR XDG_DATA_DIRS=$tmp/system XDG_CONFIG_HOME=$tmp/schemas/config XDG_STATE_HOME=$tmp/schemas/state"

info=$($senv "$sbundle/duanyan" info | grep '^shared_data_dir ')
check "info shows the bundled schemas" "$info" \
    "shared_data_dir  $(cd "$sdata" && pwd -P) (bundled rime data)"
t new-window -d -n schemas "$senv $sbundle/duanyan; sleep 600"
wait_for e2e:schemas "包内方案" 60 && echo "ok: the bundled default.yaml selects the schema"
t send-keys -t e2e:schemas -l "nihao"
wait_for e2e:schemas "包内词条" && echo "ok: the bundled dictionary is used"
check "bundled schemas stderr.log is empty" "$(cat "$tmp/schemas/state/duanyan/log/stderr.log" 2>/dev/null)" ""

step "nothing leaked to stderr"
check "stderr.log is empty" "$(cat "$tmp/state/duanyan/log/stderr.log" 2>/dev/null)" ""

if ((failures)); then
    echo "$failures failure(s)" >&2
    exit 1
fi
echo "all e2e checks passed"
