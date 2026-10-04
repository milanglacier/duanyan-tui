# duanyan shell integration for fish. Add to ~/.config/fish/config.fish:
#   duanyan init fish | source
#
# Typing the trigger ($DUANYAN_TRIGGER, default ^^) and pressing Tab opens
# duanyan; the submitted text replaces the trigger. Without the trigger, Tab
# runs what it was bound to when this script was loaded, so load it after
# other plugins that bind Tab. Setting DUANYAN_TRIGGER to '' before loading
# leaves Tab alone. duanyan_widget inserts text at the cursor and can be bound
# to any key.

# Runs duanyan and replaces the $argv[1] characters before the cursor with
# the submitted text.
function __duanyan_insert
    set -l out (command duanyan --stdout </dev/tty)
    or begin
        commandline -f repaint
        return 1
    end
    set -l text (string join \n -- $out)
    set -l buf (commandline -b | string collect)
    set -l pos (commandline -C)
    set -l before (string sub -l (math $pos - $argv[1]) -- "$buf")
    set -l after (string sub -s (math $pos + 1) -- "$buf")
    commandline -r -- "$before$text$after"
    commandline -C (string length -- "$before$text")
    commandline -f repaint
end

function duanyan_widget
    __duanyan_insert 0
end

function __duanyan_tab
    set -l trigger '^^'
    set -q DUANYAN_TRIGGER; and set trigger $DUANYAN_TRIGGER
    set -l n (string length -- "$trigger")
    set -l left (commandline -cb | string collect)
    if test $n -gt 0; and test "$(string sub -s -$n -- "$left")" = "$trigger"
        __duanyan_insert $n
        return
    end
    set -l fallback __duanyan_tab_fallback_$fish_bind_mode
    set -l sets_mode __duanyan_tab_sets_mode_$fish_bind_mode
    if not set -q $fallback
        commandline -f complete
        return
    end
    # Run the saved binding the way bind would: input functions through
    # commandline -f, anything else as a command.
    set -l input_functions (bind --function-names)
    for cmd in $$fallback
        if contains -- $cmd $input_functions
            commandline -f $cmd
        else
            eval $cmd
        end
    end
    # bind -m switches the mode after the commands have run.
    if test -n "$$sets_mode"
        set -g fish_bind_mode $$sets_mode
    end
end

if not set -q DUANYAN_TRIGGER; or test -n "$DUANYAN_TRIGGER"
    for mode in default insert
        # Preset bindings print before user ones; the last line wins. The key
        # prints as `tab` in fish 4 and `\t` in fish 3, and the remaining
        # arguments are escaped, so eval turns them back into a list.
        set -l line (bind -M $mode \t 2>/dev/null)[-1]
        set -l cmds (string replace -rf -- '^bind (--preset )?(-M \S+ )?(-m \S+ )?(tab|\\\\t) ' '' $line)
        if test -n "$cmds" -a "$cmds" != __duanyan_tab
            eval "set -g __duanyan_tab_fallback_$mode $cmds"
            set -g __duanyan_tab_sets_mode_$mode (string match -rg -- '^bind (?:--preset )?(?:-M \S+ )?-m (\S+) ' $line)
        end
        bind -M $mode \t __duanyan_tab
    end
end
