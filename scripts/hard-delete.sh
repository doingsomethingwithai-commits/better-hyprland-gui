#!/usr/bin/env bash
set -euo pipefail

APP_DIR="${APP_DIR:-$HOME/.local/share/better-hyprland-gui}"

log() {
  printf '%s\n' "$*"
}

if [[ ! -e "$APP_DIR" ]]; then
  log "Nothing to delete: $APP_DIR does not exist."
  exit 0
fi

case "$APP_DIR" in
  "$HOME"|"${HOME}/"|"")
    log "Refusing to delete an unsafe path: $APP_DIR"
    exit 1
    ;;
esac

log "This will permanently delete:"
log "  $APP_DIR"
log "Type DELETE to continue."
read -r confirmation

if [[ "$confirmation" != "DELETE" ]]; then
  log "Aborted."
  exit 1
fi

rm -rf -- "$APP_DIR"
log "Deleted $APP_DIR"
