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

update_existing_checkout() {
  local repo_dir="$1"

  if [[ -n "$APP_REF" ]]; then
    git -C "$repo_dir" fetch --prune --tags origin
    checkout_version_ref "$repo_dir" "$APP_REF"
    return 0
  fi

  local current_branch remote_branch
  ensure_clean_checkout "$repo_dir"
  current_branch="$(git -C "$repo_dir" rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
  if [[ -z "$current_branch" || "$current_branch" == "HEAD" ]]; then
    log "Refusing automatic update from detached HEAD. Set APP_REF explicitly or switch to a branch."
    return 1
  fi
  remote_branch="origin/$current_branch"

  git -C "$repo_dir" fetch --prune --tags origin
  if ! git -C "$repo_dir" show-ref --verify --quiet "refs/remotes/${remote_branch}"; then
    log "No matching remote branch found: $remote_branch"
    return 1
  fi
  git -C "$repo_dir" merge --ff-only "$remote_branch"
}

clone_or_update_repo() {
  local target_dir
  target_dir="$(resolve_target_dir)"

  if [[ -e "$target_dir/.git" ]]; then
    log "Hard-updating existing checkout in $target_dir"
    update_existing_checkout "$target_dir"
  else
    log "Checkout not found, cloning into $APP_DIR"
    git clone "$REPO_URL" "$APP_DIR"
    if [[ -n "$APP_REF" ]]; then
      git -C "$APP_DIR" fetch --tags origin
      checkout_version_ref "$APP_DIR" "$APP_REF"
    fi
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
