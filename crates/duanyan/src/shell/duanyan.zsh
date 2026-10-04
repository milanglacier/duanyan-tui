# duanyan shell integration for zsh. Add to ~/.zshrc:
#   eval "$(duanyan init zsh)"
#
# Typing the trigger ($DUANYAN_TRIGGER, default ^^) and pressing Tab opens
# duanyan; the submitted text replaces the trigger. Without the trigger, Tab
# runs the widget it was bound to when this script was loaded, so load it
# after other plugins that bind Tab. Setting DUANYAN_TRIGGER to '' before
# loading leaves Tab alone. duanyan-widget inserts text at the cursor and can
# be bound to any key.

duanyan-widget() {
  local text
  text=$(command duanyan --stdout </dev/tty) && LBUFFER+=$text
  local ret=$?
  zle reset-prompt
  return ret
}

__duanyan_tab() {
  local trigger=${DUANYAN_TRIGGER-'^^'} text keymap=$KEYMAP
  if [[ -n $trigger && $LBUFFER == *"$trigger" ]]; then
    text=$(command duanyan --stdout </dev/tty) && LBUFFER=${LBUFFER%"$trigger"}$text
    zle reset-prompt
  else
    # $KEYMAP is `main` when main is the selected keymap; look up what it
    # links to (`bindkey -A viins main`).
    [[ $keymap == main ]] && keymap=${${(z)"$(bindkey -lL main)"}[3]}
    zle ${__duanyan_tab_fallback[$keymap]:-expand-or-complete}
  fi
}

zle -N duanyan-widget

if [[ -n ${DUANYAN_TRIGGER-x} ]]; then
  zle -N __duanyan_tab
  typeset -gA __duanyan_tab_fallback
  () {
    local keymap current
    for keymap in emacs viins; do
      current=${${(z)"$(bindkey -M $keymap '^I')"}[2]}
      if [[ $current != __duanyan_tab && $current != undefined-key ]]; then
        __duanyan_tab_fallback[$keymap]=$current
      fi
      bindkey -M $keymap '^I' __duanyan_tab
    done
  }
fi
