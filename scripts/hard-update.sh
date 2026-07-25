#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/doingsomethingwithai-commits/better-hyprland-gui.git"
APP_DIR="${APP_DIR:-$HOME/.local/share/better-hyprland-gui}"
APP_REF="${APP_REF:-}"
NO_LAUNCH="${NO_LAUNCH:-0}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/hyprgui"
INSTALL_STATE_FILE="$CONFIG_DIR/install.env"

log() {
  printf '%s\n' "$*"
}

have() {
  command -v "$1" >/dev/null 2>&1
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

install_rustup_if_missing() {
  if have cargo; then
    return 0
  fi

  if have rustup; then
    log "Rustup found, installing the stable Rust toolchain."
    rustup default stable
  else
    log "Rust toolchain not found, installing rustup."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  fi

  if [[ -r "$HOME/.cargo/env" ]]; then
    # shellcheck disable=SC1090,SC1091
    source "$HOME/.cargo/env"
  fi
}

checkout_version_ref() {
  local repo_dir="$1"
  local ref="$2"
  local candidate
  local candidates=("$ref" "origin/$ref" "refs/tags/$ref")

  if [[ -z "$ref" || "$ref" == -* || "$ref" =~ [[:space:][:cntrl:]] ]]; then
    log "Refusing unsafe version ref: $ref"
    return 1
  fi

  ensure_clean_checkout "$repo_dir"

  for candidate in "${candidates[@]}"; do
    if git -C "$repo_dir" checkout --detach "$candidate" >/dev/null 2>&1; then
      return 0
    fi
  done

  git -C "$repo_dir" checkout --detach "$ref"
}

ensure_clean_checkout() {
  local repo_dir="$1"

  if [[ -n "$(git -C "$repo_dir" status --porcelain)" ]]; then
    log "Refusing to update a checkout with local changes: $repo_dir"
    log "Commit or stash the changes first."
    return 1
  fi
}

validate_checkout_for_replacement() {
  local target_dir="$1"
  local target_real home_real

  if [[ -L "$target_dir" ]]; then
    log "Refusing to replace a symlinked checkout: $target_dir"
    return 1
  fi

  if command -v realpath >/dev/null 2>&1; then
    target_real="$(realpath "$target_dir")"
    home_real="$(realpath "$HOME")"
  else
    target_real="$(cd "$target_dir" && pwd -P)"
    home_real="$(cd "$HOME" && pwd -P)"
  fi

  case "$target_real" in
    "$home_real"/*) ;;
    *)
      log "Refusing to replace a path outside the home directory: $target_real"
      return 1
      ;;
  esac

  if [[ ! -e "$target_dir/.git" || ! -f "$target_dir/Cargo.toml" ]]; then
    log "Refusing to replace a directory that is not a Better Hyprland GUI checkout: $target_dir"
    return 1
  fi

  if ! grep -Eq '^[[:space:]]*name[[:space:]]*=[[:space:]]*"hyprgui"[[:space:]]*$' "$target_dir/Cargo.toml"; then
    log "Refusing to replace a checkout whose Cargo package is not hyprgui: $target_dir"
    return 1
  fi
}

clone_or_update_repo() {
  local target_dir
  target_dir="$(resolve_target_dir)"

  if [[ -e "$target_dir" ]]; then
    validate_checkout_for_replacement "$target_dir"
    log "Removing existing checkout for hard update: $target_dir"
    rm -rf -- "$target_dir"
  fi

  mkdir -p "$(dirname "$target_dir")"
  log "Cloning a fresh checkout into $target_dir"
  git clone "$REPO_URL" "$target_dir"
  if [[ -n "$APP_REF" ]]; then
    git -C "$target_dir" fetch --tags origin
    checkout_version_ref "$target_dir" "$APP_REF"
  fi
}

build_app() {
  local target_dir
  target_dir="$(resolve_target_dir)"
  log "Rebuilding software"
  (
    cd "$target_dir"
    cargo build --release
  )
}

write_install_state() {
  local target_dir
  target_dir="$(resolve_target_dir)"

  mkdir -p "$CONFIG_DIR"
  printf 'APP_DIR=%s\nHYPRGUI_REPO_DIR=%s\n' "$target_dir" "$target_dir" > "$INSTALL_STATE_FILE"
}

launch_app() {
  if [[ "$NO_LAUNCH" == "1" ]]; then
    log "Skipping app launch because NO_LAUNCH=1."
    return 0
  fi

  local target_dir
  target_dir="$(resolve_target_dir)"
  local binary_path="$target_dir/target/release/hyprgui"
  if [[ ! -x "$binary_path" ]]; then
    log "Built binary not found at $binary_path"
    log "Skipping automatic launch."
    return 0
  fi

  log "Launching Better Hyprland GUI"
  "$binary_path"
}

main() {
  clone_or_update_repo
  install_rustup_if_missing

  if have cargo; then
    build_app
    write_install_state
  else
    log "Cargo not found, skipping rebuild."
  fi

  launch_app

  local target_dir
  target_dir="$(resolve_target_dir)"

  log ""
  log "Done."
  log "Run it with:"
  log "  \"$target_dir/target/release/hyprgui\""
  log "Or reinstall the launcher via the bootstrap script if you want the menu entry refreshed."
  if [[ -n "$APP_REF" ]]; then
    log ""
    log "Pinned ref used:"
    log "  $APP_REF"
  fi
}

main "$@"
