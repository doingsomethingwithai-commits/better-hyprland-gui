#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/doingsomethingwithai-commits/better-hyprland-gui.git"
APP_DIR="${APP_DIR:-$HOME/.local/share/better-hyprland-gui}"
APP_REF="${APP_REF:-}"
NO_LAUNCH="${NO_LAUNCH:-0}"

log() {
  printf '%s\n' "$*"
}

have() {
  command -v "$1" >/dev/null 2>&1
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

source_os_release() {
  if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091
    . /etc/os-release
  fi
}

install_rustup_if_missing() {
  if ! have cargo; then
    log "Rust toolchain not found, installing rustup."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env"
  fi
}

install_arch_deps() {
  sudo pacman -Sy --needed --noconfirm git curl base-devel gtk4 pango pkgconf
}

install_debian_deps() {
  sudo apt update
  sudo apt install -y git curl build-essential pkg-config libgtk-4-dev libpango1.0-dev
}

install_fedora_deps() {
  sudo dnf install -y git curl rustup gtk4-devel pango-devel pkgconf-pkg-config
}

install_opensuse_deps() {
  sudo zypper --non-interactive install git curl rustup gtk4-devel pango-devel pkgconf-pkg-config
}

install_nix_deps() {
  nix profile install nixpkgs#git nixpkgs#curl nixpkgs#rustup nixpkgs#gtk4 nixpkgs#pango nixpkgs#pkg-config
}

clone_or_update_repo() {
  if [[ -d "$APP_DIR/.git" ]]; then
    log "Updating existing checkout in $APP_DIR"
    if [[ -n "$APP_REF" ]]; then
      git -C "$APP_DIR" fetch --tags origin
      checkout_version_ref "$APP_REF"
    else
      git -C "$APP_DIR" pull --rebase
    fi
  else
    log "Cloning repository into $APP_DIR"
    git clone "$REPO_URL" "$APP_DIR"
    if [[ -n "$APP_REF" ]]; then
      git -C "$APP_DIR" fetch --tags origin
      checkout_version_ref "$APP_REF"
    fi
  fi
}

build_app() {
  log "Building Better Hyprland GUI"
  (
    cd "$APP_DIR"
    cargo build --release
  )
}

launch_app() {
  if [[ "$NO_LAUNCH" == "1" ]]; then
    log "Skipping app launch because NO_LAUNCH=1."
    return 0
  fi

  local binary_path="$APP_DIR/target/release/hyprgui"
  if [[ ! -x "$binary_path" ]]; then
    log "Built binary not found at $binary_path"
    log "Skipping automatic launch."
    return 0
  fi

  log "Launching Better Hyprland GUI"
  "$binary_path"
}

main() {
  source_os_release

  case "${ID:-unknown}" in
    arch|manjaro|endeavouros|athena|athenaos)
      install_arch_deps
      install_rustup_if_missing
      ;;
    fedora)
      install_fedora_deps
      install_rustup_if_missing
      ;;
    opensuse*|suse)
      install_opensuse_deps
      install_rustup_if_missing
      ;;
    ubuntu|debian)
      install_debian_deps
      install_rustup_if_missing
      ;;
    nixos)
      install_nix_deps
      install_rustup_if_missing
      ;;
    *)
      log "Unsupported distro: ${ID:-unknown}"
      log "Hyprland is officially tested on Arch Linux and NixOS."
      log "Open the Hyprland installation page in the GUI for the safest next step."
      exit 1
      ;;
  esac

  clone_or_update_repo
  build_app
  launch_app
  log ""
  log "Done."
  log "If you want to launch it manually later:"
  log "  \"$APP_DIR/target/release/hyprgui\""
  log ""
  log "If you want to stay on the installed checkout:"
  log "  cd \"$APP_DIR\""
  if [[ -n "$APP_REF" ]]; then
    log ""
    log "Pinned ref used:"
    log "  $APP_REF"
  fi
}

main "$@"
