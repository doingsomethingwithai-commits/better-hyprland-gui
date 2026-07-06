#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/doingsomethingwithai-commits/better-hyprland-gui.git"
APP_DIR="${APP_DIR:-$HOME/.local/share/better-hyprland-gui}"
INSTALL_HYPRLAND="${INSTALL_HYPRLAND:-0}"

log() {
  printf '%s\n' "$*"
}

have() {
  command -v "$1" >/dev/null 2>&1
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

install_hyprland_arch() {
  sudo pacman -S --needed --noconfirm hyprland
}

install_hyprland_nix() {
  nix profile install nixpkgs#hyprland
}

clone_or_update_repo() {
  if [[ -d "$APP_DIR/.git" ]]; then
    log "Updating existing checkout in $APP_DIR"
    git -C "$APP_DIR" pull --rebase
  else
    log "Cloning repository into $APP_DIR"
    git clone "$REPO_URL" "$APP_DIR"
  fi
}

build_app() {
  log "Building Better Hyprland GUI"
  (
    cd "$APP_DIR"
    cargo build --release
  )
}

main() {
  source_os_release

  case "${ID:-unknown}" in
    arch|manjaro|endeavouros|athena|athenaos)
      install_arch_deps
      install_rustup_if_missing
      if [[ "$INSTALL_HYPRLAND" == "1" ]]; then
        install_hyprland_arch
      fi
      ;;
    fedora)
      install_fedora_deps
      install_rustup_if_missing
      if [[ "$INSTALL_HYPRLAND" == "1" ]]; then
        log "Hyprland package names vary on Fedora setups. Open the GUI after install for the distro guidance page."
      fi
      ;;
    opensuse*|suse)
      install_opensuse_deps
      install_rustup_if_missing
      if [[ "$INSTALL_HYPRLAND" == "1" ]]; then
        log "Hyprland package names vary on openSUSE setups. Open the GUI after install for the distro guidance page."
      fi
      ;;
    ubuntu|debian)
      install_debian_deps
      install_rustup_if_missing
      if [[ "$INSTALL_HYPRLAND" == "1" ]]; then
        log "Hyprland is not packaged consistently on Debian/Ubuntu. Follow the GUI's install page for the supported path."
      fi
      ;;
    nixos)
      install_nix_deps
      install_rustup_if_missing
      if [[ "$INSTALL_HYPRLAND" == "1" ]]; then
        install_hyprland_nix
      fi
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
  log ""
  log "Done."
  log "Run it with:"
  log "  cargo run --release"
  log ""
  log "If you want to stay on the installed checkout:"
  log "  cd \"$APP_DIR\""
}

main "$@"
