# Notes for coding agents

## Build and test

Run everything inside the devShell, which provides the Rust toolchain,
librime, rime-data and tmux, and exports `DUANYAN_LIBRIME_PATH`,
`DUANYAN_RIME_SHARED_DIR` and `RIME_INCLUDE_DIR`:

```sh
nix develop -c cargo test             # unit, FFI layout and librime integration tests
nix develop -c cargo clippy --all-targets
nix develop -c scripts/e2e.sh         # end-to-end tests in a real terminal (tmux)
nix build                             # package; its check phase runs the cargo tests
```

`cargo test` never opens a terminal. Any change to terminal handling,
rendering or key routing must also pass `scripts/e2e.sh`.

## End-to-end testing with tmux

`scripts/e2e.sh` runs the real binary in tmux panes: tmux provides a real pty,
answers terminal queries, turns `send-keys` into input, and records OSC 52 in
`tmux show-buffer`. It uses a private tmux server and temporary XDG dirs, so
the user's configuration is never touched. Extend it when adding behavior.

For ad-hoc checks, follow the same pattern:

```sh
tmp=$(mktemp -d)
t() { tmux -L duanyan-dev -f /dev/null "$@"; }
t new-session -d -s dev -x 100 -y 30 \
  "XDG_CONFIG_HOME=$tmp/config XDG_STATE_HOME=$tmp/state target/debug/duanyan; echo EXIT=\$?; sleep 600"
t set -s set-clipboard on
t send-keys -t dev -l "nihao"     # literal text
t send-keys -t dev C-j M-l Enter  # named keys
t capture-pane -p -t dev          # read the screen
t kill-server
```

- The first run deploys rime; wait for the schema name in the header before
  typing. To make the deploy fast and the candidates predictable, put a
  `default.custom.yaml` with a one-schema `schema_list` in
  `$tmp/config/duanyan/rime`, as the script does.
- Test `--stdout` from a shell pane with `out=$(duanyan --stdout)` and
  print `$?` and `$out`. The inline UI draws on `/dev/tty`, so stdout must
  contain only the submitted text.
- Poll `capture-pane` until the expected text appears; do not use fixed
  sleeps.

tmux cannot cover everything:

- **Kitty keyboard protocol**: tmux does not forward lone modifier keys, so a
  bare Shift_L/Shift_R toggling ascii mode can only be verified by hand in
  kitty (or another KKP terminal) without a multiplexer. herdr has the same
  limitation (see the plan's `progress.md`).
- **Mouse clicks**: not exercised by the script. The hit-testing logic is
  unit-tested in `app.rs`, and clicks were verified by hand in kitty.

## Terminal pitfalls

- Never write to stdout from the TUI; `--stdout` owns it. Terminal output and
  queries (KKP, OSC 11, cursor position) go through `/dev/tty`. crossterm's
  `supports_keyboard_enhancement()` and `cursor::position()` write to stdout,
  so do not use them.
- librime logs through glog. Its log directory must exist before
  initialization, and glog copies ERROR logs to stderr, which the TUI
  redirects to `$XDG_STATE_HOME/duanyan/log/stderr.log`. After an e2e run,
  that file should be empty.

## Plans

Design plans live in `.plans/{active,completed}/<name>/plan.md`, with
implementation progress in `progress.md` next to each plan.
