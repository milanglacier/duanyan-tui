# Notes for coding agents

## Build and test

Run everything inside the devShell, which provides the Rust toolchain,
librime, rime-data, tmux and the shells for the integration tests
(zsh, fish, bash), and exports `DUANYAN_LIBRIME_PATH`,
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

`scripts/e2e.sh` runs the real binary in tmux, which answers the queries
duanyan sends to the terminal (OSC 11, KKP, cursor position) the way a real
terminal does. Extend the script when adding behavior.

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
  typing. Copy the script's one-schema `default.custom.yaml` into
  `$tmp/config/duanyan/rime` to keep the deploy fast.
- Poll `capture-pane` until the expected text appears; do not use fixed
  sleeps.
- **Kitty keyboard protocol**: tmux does not forward lone modifier keys
  (herdr has the same limitation). To test them, force
  `kitty_keyboard = "on"` and write the CSI u sequences a KKP terminal would
  send with `send-keys -l`, as `scripts/e2e.sh` does. Whether a real terminal
  sends those sequences can only be checked by hand in kitty (or another KKP
  terminal) without a multiplexer.

## Terminal pitfalls

- Never write to stdout from the TUI; `--stdout` owns it. Terminal output and
  queries (KKP, OSC 11, cursor position) go through `/dev/tty`. crossterm's
  `supports_keyboard_enhancement()` and `cursor::position()` write to stdout,
  so do not use them.
- librime logs through glog. Its log directory must exist before
  initialization, and glog copies ERROR logs to stderr, which the TUI
  redirects to `$XDG_STATE_HOME/duanyan/log/stderr.log`. After an e2e run,
  that file should be empty.
- Under KKP a chord such as `ctrl+l` arrives as separate events: a bare Ctrl
  press, then `l`. librime's `ascii_composer` counts a Shift tap only when no
  other modifier went down first, so a Shift tap synthesized for that chord
  does nothing if rime has already seen the Ctrl press.

## Bundled librime

The release workflow also publishes `-bundled` archives. macOS uses librime's
own universal build (version and sha256 are pinned in
`.github/workflows/release.yml`). librime publishes no Linux build, so
`scripts/build-librime.sh` compiles a self-contained `librime.so.1` on Ubuntu
22.04. When bumping librime, update the versions in both files.

## Plans

Design plans live in `.plans/{active,completed}/<name>/plan.md`, with
implementation progress in `progress.md` next to each plan.

## Documentation

- `README.md` is for users: installing, configuring and using duanyan. Keep
  implementation details out of it, such as what the release archives bundle
  internally, lookup internals or build flags.
- Update AGENTS.md (symlinked to CLAUDE.md) for nontrivial decisions and for
  anything the code and scripts cannot explain themselves, such as pitfalls,
  constraints or how to test. Do not restate what the code or its comments
  already say.
