# duanyan popup: the inline UI in a tmux, zellij or herdr popup next to the
# cursor of the pane underneath; the submitted text is pasted into that pane.
# Run by `duanyan popup <mux> [args...]` as `sh -c <this> duanyan <exe> <mux>
# [args...]`, so that it always runs the duanyan binary it came with.
#
# DUANYAN_POPUP_WIDTH and DUANYAN_POPUP_HEIGHT size the popup in tmux and
# zellij. The default height fits horizontal candidates; vertical candidates
# need 3 + page_size + 1 rows.

exe=$1 mux=$2
shift 2
width=${DUANYAN_POPUP_WIDTH:-60}
height=${DUANYAN_POPUP_HEIGHT:-5}
# Inside a popup an error would vanish with it; wait for a key instead.
wait_on_error=

die() {
  printf 'duanyan popup: %s\n' "$1" >&2
  if [ -n "$wait_on_error" ]; then
    printf 'Press Enter to close.' >&2
    read -r _ </dev/tty
  fi
  exit 1
}

# place SX SY SW SH PX PY PW PH CX CY W H
# Prints "X Y W H" for a W x H popup on the screen area (SX, SY, SW, SH),
# next to the cursor (CX, CY) of the pane content area (PX, PY, PW, PH):
# below the cursor when there is room, else above it, with the text column
# on the cursor (the inline UI has a one-cell margin). Without a usable
# cursor (CX is -1, or it is outside the pane, as with TUIs that hide it and
# draw their own), the popup sits at the pane's bottom left.
place() {
  sx=$1 sy=$2 sw=$3 sh=$4 px=$5 py=$6 pw=$7 ph=$8 cx=$9 cy=${10} w=${11} h=${12}
  [ "$w" -gt "$sw" ] && w=$sw
  [ "$h" -gt "$sh" ] && h=$sh
  sr=$((sx + sw)) sb=$((sy + sh))
  if [ "$cx" -ge "$px" ] && [ "$cx" -lt $((px + pw)) ] &&
    [ "$cy" -ge "$py" ] && [ "$cy" -lt $((py + ph)) ] &&
    [ "$cx" -ge "$sx" ] && [ "$cx" -lt "$sr" ] &&
    [ "$cy" -ge "$sy" ] && [ "$cy" -lt "$sb" ]; then
    x=$((cx > 0 ? cx - 1 : 0))
    if [ $((cy + 1 + h)) -le "$sb" ]; then
      y=$((cy + 1))
    elif [ "$cy" -ge $((sy + h)) ]; then
      y=$((cy - h))
    else
      y=$((sb - h))
    fi
  else
    x=$px
    y=$((py + ph < sb ? py + ph : sb))
    y=$((y - h > sy ? y - h : sy))
  fi
  [ "$x" -gt $((sr - w)) ] && x=$((sr - w))
  [ "$x" -lt "$sx" ] && x=$sx
  echo "$x $y $w $h"
}

# Runs duanyan; on submit, the text is in $tmp and the status is 0. A cancel
# returns 1; any other failure is an error, which stays on screen.
input() {
  tmp=$(mktemp "${TMPDIR:-/tmp}/duanyan-popup.XXXXXX") || die 'mktemp failed'
  trap 'rm -f "$tmp"' EXIT
  status=0
  "$exe" --stdout <"/dev/tty" >"$tmp" || status=$?
  [ "$status" -le 1 ] || die "duanyan exited with status $status"
  return "$status"
}

# The text in file $1 as a bracketed paste, so that its newlines do not act
# as Enter. ESC is dropped: it could end the paste early.
bracketed() {
  text=$(tr -d '\033' <"$1"; printf x)
  printf '\033[200~%s\033[201~' "${text%x}"
}

quote() {
  printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

case $mux in
place)
  place "$@"
  ;;
tmux)
  if [ "$1" = --inside ]; then
    # In the popup, opened by the branch below.
    wait_on_error=1
    pane=$2
    input || exit 0
    buffer=duanyan-$$
    if ! tmux load-buffer -b "$buffer" "$tmp" ||
      ! tmux paste-buffer -p -d -b "$buffer" -t "$pane"; then
      die 'pasting failed'
    fi
    exit 0
  fi
  # From `run-shell`, which has neither a current pane nor a client.
  pane=$1 client=$2
  [ -n "$pane" ] && [ -n "$client" ] ||
    die 'usage: duanyan popup tmux "#{pane_id}" "#{client_name}"'
  # shellcheck disable=SC2046
  set -- $(tmux display -p -c "$client" -t "$pane" \
    '#{pane_left} #{pane_top} #{pane_width} #{pane_height} #{cursor_x} #{cursor_y} #{cursor_flag} #{window_width} #{window_height} #{client_width} #{client_height} #{status} #{status-position}')
  [ $# -eq 13 ] || die "cannot read the geometry of pane $pane"
  pl=$1 pt=$2 pw=$3 ph=$4 cx=$5 cy=$6 shown=$7 ww=$8 wh=$9 cw=${10} ch=${11}
  case ${12} in
  off) lines=0 ;;
  on) lines=1 ;;
  *) lines=${12} ;;
  esac
  # A status line on top pushes the window down.
  top=0
  [ "${13}" = top ] && top=$lines
  pt=$((pt + top))
  ch=$((ch - lines))
  if [ "$shown" = 1 ]; then
    cx=$((pl + cx)) cy=$((pt + cy))
  else
    cx=-1 cy=-1
  fi
  # shellcheck disable=SC2046
  set -- $(place 0 "$top" $((ww < cw ? ww : cw)) $((wh < ch ? wh : ch)) \
    "$pl" "$pt" "$pw" "$ph" "$cx" "$cy" "$width" "$height")
  # -y is the popup's bottom edge.
  tmux display-popup -c "$client" -t "$pane" -B -x "$1" -y $(($2 + $4)) \
    -w "$3" -h "$4" -E "$(quote "$exe") popup tmux --inside $(quote "$pane")"
  ;;
zellij)
  wait_on_error=1
  command -v jq >/dev/null || die 'zellij support needs jq'
  case $ZELLIJ_PANE_ID in
  '' | *[!0-9]*) die 'run this from a zellij floating pane' ;;
  esac
  # In a floating pane, the tab's focused tiled pane still reports
  # is_focused: that is the pane the popup was opened from. Floating panes
  # can use the area the tiled panes cover.
  # shellcheck disable=SC2046
  set -- $(zellij action list-panes -a -j | jq -r --argjson me "$ZELLIJ_PANE_ID" '
    (map(select((.is_plugin | not) and .id == $me))[0].tab_id) as $tab
    | map(select(.tab_id == $tab)) as $panes
    | ($panes | map(select((.is_plugin | not) and (.is_floating | not) and .is_focused))[0]) as $t
    | ($panes | map(select((.is_floating | not) and .is_selectable and (.is_suppressed | not)))) as $tiled
    | if $t == null then empty else
        ($tiled | map(.pane_x) | min) as $x | ($tiled | map(.pane_y) | min) as $y
        | [$t.id, $x, $y,
           ($tiled | map(.pane_x + .pane_columns) | max) - $x,
           ($tiled | map(.pane_y + .pane_rows) | max) - $y,
           $t.pane_content_x, $t.pane_content_y, $t.pane_content_columns, $t.pane_content_rows,
           (($t.cursor_coordinates_in_pane // null) as $c
            | if $c then $t.pane_content_x + $c[0], $t.pane_content_y + $c[1] else -1, -1 end)]
        | map(tostring) | join(" ")
      end')
  [ $# -eq 11 ] || die 'cannot find the pane to type into'
  target=terminal_$1
  shift
  # shellcheck disable=SC2046
  set -- $(place "$@" "$width" "$height")
  zellij action change-floating-pane-coordinates --pane-id "terminal_$ZELLIJ_PANE_ID" \
    --x "$1" --y "$2" --width "$3" --height "$4" --borderless true ||
    die 'cannot move the floating pane'
  # zellij resizes the pane asynchronously; duanyan reads the size once.
  i=0
  while [ "$(stty size </dev/tty)" != "$4 $3" ] && [ $i -lt 50 ]; do
    sleep 0.02
    i=$((i + 1))
  done
  input || exit 0
  zellij action write-chars --pane-id "$target" "$(bracketed "$tmp")" ||
    die 'pasting failed'
  ;;
herdr)
  wait_on_error=1
  [ -n "$HERDR_ACTIVE_PANE_ID" ] || die 'run this from a herdr popup'
  input || exit 0
  herdr pane send-text "$HERDR_ACTIVE_PANE_ID" "$(bracketed "$tmp")" >/dev/null ||
    die 'pasting failed'
  ;;
*)
  die "unknown multiplexer: $mux"
  ;;
esac
