# duanyan shell integration for fish. Add to ~/.config/fish/config.fish:
#   duanyan init fish | source
#
# Typing the trigger ($DUANYAN_TRIGGER, default ^^) and pressing Tab opens
# duanyan; the submitted text replaces the trigger. Without the trigger, Tab
# completes as usual. duanyan_widget inserts text at the cursor and can be
# bound to any key.

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
    else
        commandline -f complete
    end
end

bind \t __duanyan_tab
bind -M insert \t __duanyan_tab
