#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/doingsomethingwithai-commits/better-hyprland-gui.git"
APP_DIR="${APP_DIR:-$HOME/.local/share/better-hyprland-gui}"
APP_REF="${APP_REF:-}"

log() {
  printf '%s\n' "$*"
}

checkout_version_ref() {
  local ref="$1"
  local candidate
  local candidates=("$ref" "origin/$ref" "refs/tags/$ref")

  for candidate in "${candidates[@]}"; do
    if git -C "$APP_DIR" checkout --force "$candidate" >/dev/null 2>&1; then
      return 0
    fi
  done

  git -C "$APP_DIR" checkout --force "$ref"
}

if [[ -d "$APP_DIR/.git" ]]; then
  log "Hard-updating existing checkout in $APP_DIR"
  if [[ -n "$APP_REF" ]]; then
    git -C "$APP_DIR" fetch --tags origin
    checkout_version_ref "$APP_REF"
  else
    git -C "$APP_DIR" fetch origin main
    git -C "$APP_DIR" reset --hard origin/main
  fi
else
  log "Checkout not found, cloning into $APP_DIR"
  git clone "$REPO_URL" "$APP_DIR"
  if [[ -n "$APP_REF" ]]; then
    git -C "$APP_DIR" fetch --tags origin
    checkout_version_ref "$APP_REF"
  fi
fi

if command -v cargo >/dev/null 2>&1; then
  log "Rebuilding software"
  (
    cd "$APP_DIR"
    cargo build --release
  )
else
  log "Cargo not found, skipping rebuild."
fi

log ""
log "Done."
log "Run it with:"
log "  cd \"$APP_DIR\""
log "  cargo run --release"
if [[ -n "$APP_REF" ]]; then
  log ""
  log "Pinned ref used:"
  log "  $APP_REF"
fi
