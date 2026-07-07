#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/doingsomethingwithai-commits/better-hyprland-gui.git"
APP_DIR="${APP_DIR:-$HOME/.local/share/better-hyprland-gui}"
APP_REF="${APP_REF:-}"
NO_LAUNCH="${NO_LAUNCH:-0}"
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
DESKTOP_FILE_NAME="hyprgui.desktop"

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

checkout_version_ref() {
  local repo_dir="$1"
  local ref="$2"
  local candidate
  local candidates=("$ref" "origin/$ref" "refs/tags/$ref")

  for candidate in "${candidates[@]}"; do
    if git -C "$repo_dir" checkout --force "$candidate" >/dev/null 2>&1; then
      return 0
    fi
  done

  git -C "$repo_dir" checkout --force "$ref"
}

update_existing_checkout() {
  local repo_dir="$1"

  if [[ -n "$APP_REF" ]]; then
    git -C "$repo_dir" fetch --prune --tags origin
    checkout_version_ref "$repo_dir" "$APP_REF"
    return 0
  fi

  local current_branch remote_branch
  current_branch="$(git -C "$repo_dir" rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
  if [[ -n "$current_branch" && "$current_branch" != "HEAD" ]]; then
    remote_branch="origin/$current_branch"
  else
    remote_branch="origin/main"
  fi

  git -C "$repo_dir" fetch --prune --tags origin
  if git -C "$repo_dir" show-ref --verify --quiet "refs/remotes/${remote_branch}"; then
    git -C "$repo_dir" reset --hard "$remote_branch"
  elif git -C "$repo_dir" show-ref --verify --quiet refs/remotes/origin/main; then
    git -C "$repo_dir" reset --hard origin/main
  else
    git -C "$repo_dir" reset --hard HEAD
  fi
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
  local target_dir
  target_dir="$(resolve_target_dir)"

  if [[ -d "$target_dir/.git" ]]; then
    log "Updating existing checkout in $target_dir"
    if [[ -n "$APP_REF" ]]; then
      git -C "$target_dir" fetch --prune --tags origin
      checkout_version_ref "$target_dir" "$APP_REF"
    else
      update_existing_checkout "$target_dir"
    fi
  else
    log "Cloning repository into $APP_DIR"
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
  log "Building Better Hyprland GUI"
  (
    cd "$target_dir"
    cargo build --release
  )
}

install_desktop_entry() {
  local target_dir
  target_dir="$(resolve_target_dir)"
  local binary_path="$target_dir/target/release/hyprgui"
  local desktop_path="$DESKTOP_DIR/$DESKTOP_FILE_NAME"

  if [[ ! -x "$binary_path" ]]; then
    log "Skipping desktop entry installation because the binary is missing."
    return 0
  fi

  mkdir -p "$DESKTOP_DIR"
  cat > "$desktop_path" <<EOF
[Desktop Entry]
Name=Better Hyprland GUI
Comment=GUI for configuring Hyprland, dotfiles, and updates
Exec=$binary_path
TryExec=$binary_path
Icon=preferences-system
Type=Application
Terminal=false
Categories=Settings;Utility;
Keywords=Hyprland;Wayland;dotfiles;configuration;settings;
StartupNotify=true
StartupWMClass=hyprgui
NoDisplay=false
EOF

  log "Installed desktop entry to $desktop_path"

  if have update-desktop-database; then
    update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
  fi

  if have xdg-desktop-menu; then
    xdg-desktop-menu forceupdate >/dev/null 2>&1 || true
  fi
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
  install_desktop_entry
  launch_app
  log ""
  log "Done."
  log "If you want to launch it manually later:"
  log "  \"$(resolve_target_dir)/target/release/hyprgui\""
  log ""
  log "If you want to stay on the installed checkout:"
  log "  cd \"$(resolve_target_dir)\""
  if [[ -n "$APP_REF" ]]; then
    log ""
    log "Pinned ref used:"
    log "  $APP_REF"
  fi
}

main "$@"
