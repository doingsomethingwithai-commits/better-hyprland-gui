#!/usr/bin/env bash
set -euo pipefail

APP_DIR="${APP_DIR:-$HOME/.local/share/better-hyprland-gui}"

log() {
  printf '%s\n' "$*"
}

find_repo_root() {
  local start_dir="$1"
  local current_dir="$start_dir"

  while [[ -n "$current_dir" && "$current_dir" != "/" ]]; do
    if [[ -d "$current_dir/.git" ]]; then
      printf '%s\n' "$current_dir"
      return 0
    fi
    current_dir="$(dirname "$current_dir")"
  done

  return 1
}

resolve_target_dir() {
  if find_repo_root "$PWD" >/dev/null 2>&1; then
    find_repo_root "$PWD"
    return 0
  fi

  printf '%s\n' "$APP_DIR"
}

TARGET_DIR="$(resolve_target_dir)"

if [[ ! -e "$TARGET_DIR" ]]; then
  log "Nothing to delete: $TARGET_DIR does not exist."
  exit 0
fi

case "$TARGET_DIR" in
  "$HOME"|"${HOME}/"|"/"|""|".")
    log "Refusing to delete an unsafe path: $TARGET_DIR"
    exit 1
    ;;
esac

rm -rf -- "$TARGET_DIR"
log "Deleted $TARGET_DIR"
