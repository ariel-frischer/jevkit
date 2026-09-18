#!/bin/sh
# jevkit Uninstaller
set -eu

BINARY_NAME="jevkit"
INSTALL_DIR="${JEVKIT_INSTALL_DIR:-$HOME/.local/bin}"
TARGET="${INSTALL_DIR}/${BINARY_NAME}"

# Colors (disabled if not a terminal)
if [ -t 1 ]; then
    RED='\e[0;31m'
    GREEN='\e[0;32m'
    YELLOW='\e[0;33m'
    NC='\e[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    NC=''
fi

if [ ! -f "$TARGET" ]; then
    printf '%bError:%b %s not found at %s\n' "${RED}" "${NC}" "$BINARY_NAME" "$TARGET" >&2
    exit 1
fi

printf 'Found %s at %s\n' "$BINARY_NAME" "$TARGET"
printf 'Remove this binary? [y/N] '
read -r answer
case "$answer" in
    y|Y|yes|YES) ;;
    *) echo "Aborted."; exit 0 ;;
esac

rm -f "$TARGET"

# Clean up backups
backup_count=0
for f in "${TARGET}.backup."*; do
    [ -e "$f" ] || continue
    rm -f "$f"
    backup_count=$((backup_count + 1))
done

printf '%b==>%b %s has been removed from %s\n' "${GREEN}" "${NC}" "$BINARY_NAME" "$INSTALL_DIR"
if [ "$backup_count" -gt 0 ]; then
    printf '%b==>%b Cleaned up %d backup(s)\n' "${GREEN}" "${NC}" "$backup_count"
fi
