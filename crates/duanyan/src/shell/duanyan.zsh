# duanyan shell integration for zsh. Add to ~/.zshrc:
#   eval "$(duanyan init zsh)"
#
# Typing the trigger ($DUANYAN_TRIGGER, default ^^) and pressing Tab opens
# duanyan; the submitted text replaces the trigger. Without the trigger, Tab
# runs the widget it was bound to before. duanyan-widget inserts text at the
# cursor and can be bound to any key.

duanyan-widget() {
  local text
  text=$(command duanyan --stdout </dev/tty) && LBUFFER+=$text
  local ret=$?
  zle reset-prompt
  return ret
}

__duanyan_tab() {
  local trigger=${DUANYAN_TRIGGER-'^^'} text
  if [[ -n $trigger && $LBUFFER == *"$trigger" ]]; then
    text=$(command duanyan --stdout </dev/tty) && LBUFFER=${LBUFFER%"$trigger"}$text
    zle reset-prompt
  else
    zle ${__duanyan_tab_fallback:-expand-or-complete}
  fi
}

zle -N duanyan-widget
zle -N __duanyan_tab

() {
  local current=${${(z)"$(bindkey '^I')"}[2]}
  if [[ $current != __duanyan_tab && $current != undefined-key ]]; then
    typeset -g __duanyan_tab_fallback=$current
  fi
}
bindkey -M emacs '^I' __duanyan_tab
bindkey -M viins '^I' __duanyan_tab
