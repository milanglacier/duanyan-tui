# duanyan shell integration for bash. Add to ~/.bashrc:
#   eval "$(duanyan init bash)"
#   bind -x '"\C-x\C-d": __duanyan_widget'
#
# __duanyan_widget opens duanyan and inserts the submitted text at the
# cursor. Unlike zsh and fish, Tab is left alone: a readline macro that runs
# a shell function before `complete` breaks listing completions on a second
# Tab.

__duanyan_widget() {
  local text
  text=$(command duanyan --stdout </dev/tty) || return
  READLINE_LINE=${READLINE_LINE:0:READLINE_POINT}$text${READLINE_LINE:READLINE_POINT}
  READLINE_POINT=$((READLINE_POINT + ${#text}))
}
