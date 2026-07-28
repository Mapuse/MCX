#!/bin/sh
# mcx-command-not-found - Shell hook for mcx command-not-found suggestions
# Source this in your shell rc file (e.g., .bashrc, .zshrc):
#   . /usr/share/mcx/command-not-found.sh
#
# Or add to /etc/profile.d/mcx-command-not-found.sh

mcx_command_not_found() {
    local cmd="$1"
    if [ -z "$cmd" ]; then
        return 127
    fi

    mcx --command-not-found "$cmd" 2>/dev/null
    return $?
}

# For bash:
if [ -n "${BASH_VERSION:-}" ]; then
    command_not_found_handle() {
        mcx_command_not_found "$1"
        return $?
    }
fi

# For zsh:
if [ -n "${ZSH_VERSION:-}" ]; then
    command_not_found_handler() {
        mcx_command_not_found "$1"
        return $?
    }
fi

# For Context (ctxrc):
if [ -n "${CONTEXT_VERSION:-}" ]; then
    command_not_found_handler() {
        mcx_command_not_found "$1"
        return $?
    }
fi
