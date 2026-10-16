#!/bin/sh
# Copy into caller-private packaging storage without linking back to inputs.
# Darwin -c requests clonefile only with explicit permission preservation:
# cloning without -p can bypass umask on a new file. Non-preserving forms use
# ordinary cp. cp falls back to copyfile when cloning is unavailable, so
# capacity savings are not assured. -Rp preserves directory modes too; keep
# caller's outer mode-0700 boundary outside the copied directory tree.
# Existing destination contents use ordinary cp: clonefile may replace a
# destination symlink rather than following it as ordinary cp would.
# Caller still owns destination/alias checks and provenance verification.
set -eu

copy_flag=
case $# in
    2) ;;
    3)
        case "$1" in
            -p|-R|-Rp) copy_flag=$1; shift ;;
            *) echo "usage: copy-private.sh [-p|-R|-Rp] SOURCE DEST" >&2; exit 2 ;;
        esac
        ;;
    *) echo "usage: copy-private.sh [-p|-R|-Rp] SOURCE DEST" >&2; exit 2 ;;
esac

copy_destination=$2
if [ -n "$copy_flag" ]; then
    set -- "$copy_flag" -- "$1" "$2"
else
    set -- -- "$1" "$2"
fi

clone_destination_available() {
    # Keep overwrite, hardlink and destination-symlink semantics with cp.
    [ ! -L "$1" ] || return 1
    [ -e "$1" ] || return 0
    [ -d "$1" ] && [ -r "$1" ] && [ -x "$1" ] || return 1
    for entry in "$1"/* "$1"/.[!.]* "$1"/..?*; do
        if [ -e "$entry" ] || [ -L "$entry" ]; then
            return 1
        fi
    done
    return 0
}

copy_platform=$(uname -s)
case "$copy_platform:$copy_flag" in
    Darwin:-p|Darwin:-Rp)
        if clone_destination_available "$copy_destination"; then
            exec /bin/cp -c "$@"
        fi
        ;;
esac
exec cp "$@"
