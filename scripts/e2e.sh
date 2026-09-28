#!/usr/bin/env bash
# End-to-end tests: runs the real duanyan binary in tmux panes (a real pty
# and terminal emulator), sends keys, and asserts on the rendered screen,
# the tmux clipboard (OSC 52) and --stdout output.
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

cat >"$tmp/duanyan" <<EOF
#!/usr/bin/env bash
export XDG_CONFIG_HOME="$tmp/config" XDG_STATE_HOME="$tmp/state"
export DUANYAN_LIBRIME_PATH="$DUANYAN_LIBRIME_PATH"
export DUANYAN_RIME_SHARED_DIR="$DUANYAN_RIME_SHARED_DIR"
exec "$bin" "\$@"
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

step "fullscreen: compat alt+l toggles ascii mode"
t send-keys -t e2e:full M-l
wait_for e2e:full " 西 " && echo "ok: status bar shows ascii mode"
t send-keys -t e2e:full -l "abc"
wait_for e2e:full "│ abc" && echo "ok: ascii text goes straight to the buffer"
t send-keys -t e2e:full M-l
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

step "nothing leaked to stderr"
check "stderr.log is empty" "$(cat "$tmp/state/duanyan/log/stderr.log" 2>/dev/null)" ""

if ((failures)); then
    echo "$failures failure(s)" >&2
    exit 1
fi
echo "all e2e checks passed"
