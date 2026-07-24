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
    if [[ -e "$current_dir/.git" ]]; then
      printf '%s\n' "$current_dir"
      return 0
    fi
    current_dir="$(dirname "$current_dir")"
  done

  return 1
}

script_dir() {
  local source="${BASH_SOURCE[0]:-}"

  if [[ -z "$source" || ! -f "$source" ]]; then
    return 1
  fi

  cd "$(dirname "$source")" && pwd
}

resolve_target_dir() {
  local source_dir

  if source_dir="$(script_dir)"; then
    if find_repo_root "$source_dir" >/dev/null 2>&1; then
      find_repo_root "$source_dir"
      return 0
    fi
  fi

  printf '%s\n' "$APP_DIR"
}

TARGET_DIR="$(resolve_target_dir)"

if [[ ! -e "$TARGET_DIR" ]]; then
  log "Nothing to delete: $TARGET_DIR does not exist."
  exit 0
fi

if command -v realpath >/dev/null 2>&1; then
  TARGET_DIR="$(realpath "$TARGET_DIR")"
  HOME_DIR="$(realpath "$HOME")"
else
  TARGET_DIR="$(cd "$TARGET_DIR" && pwd -P)"
  HOME_DIR="$(cd "$HOME" && pwd -P)"
fi

case "$TARGET_DIR" in
  "$HOME_DIR"/*) ;;
  *)
    log "Refusing to delete a path outside the home directory: $TARGET_DIR"
    exit 1
    ;;
esac

if [[ ! -e "$TARGET_DIR/.git" || ! -f "$TARGET_DIR/Cargo.toml" ]]; then
  log "Refusing to delete a directory that is not a Better Hyprland GUI checkout: $TARGET_DIR"
  exit 1
fi

if ! grep -Eq '^[[:space:]]*name[[:space:]]*=[[:space:]]*"hyprgui"[[:space:]]*$' "$TARGET_DIR/Cargo.toml"; then
  log "Refusing to delete a checkout whose Cargo package is not hyprgui: $TARGET_DIR"
  exit 1
fi

rm -rf -- "$TARGET_DIR"
log "Deleted $TARGET_DIR"
