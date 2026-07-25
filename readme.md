<div align="center">

<h1>Better Hyprland GUI</h1>

<p>A GTK4 and Rust helper for configuring Hyprland, onboarding dotfiles, and guiding Linux setup.</p>

![Preview](.github/preview.png)

</div>

> This project is a personal rework of the original [HyprGUI repository by MarkusVolk](https://github.com/MarkusVolk/hyprgui). It is not the original repository; this fork/rework maintains its own changes and direction.

## What it does

- Edit Hyprland configuration through a desktop UI.
- Browse, preview, and install dotfiles from one combined `.files` workspace.
- Open a dedicated page for Hyprland installation and update guidance on Linux.
- Install and update Hyprland directly from the GUI, and update the software itself from GitHub when needed.

## Quick Install

Use this to install the GUI and its local dependencies. After the build finishes, the script launches the app automatically unless you set `NO_LAUNCH=1`:
It also installs a desktop launcher entry into your user applications folder, so the GUI should appear in your app menu.

> [!IMPORTANT]
> Review downloaded install scripts before piping them to a shell. The command below downloads the bootstrap script from `main` and may request administrator privileges to install system packages.

```bash
curl -fsSL https://raw.githubusercontent.com/doingsomethingwithai-commits/better-hyprland-gui/main/scripts/bootstrap.sh | bash
```

To pin a specific repository version during install, set `APP_REF` to a branch, tag, or commit SHA:

```bash
APP_REF=v0.1.0 curl -fsSL https://raw.githubusercontent.com/doingsomethingwithai-commits/better-hyprland-gui/main/scripts/bootstrap.sh | bash
```

`APP_REF` pins the cloned application checkout. The bootstrap script itself still comes from `main`; for a fully pinned installation, download `scripts/bootstrap.sh` from the same tag or commit and inspect it before running it.

To skip automatic launch:

```bash
NO_LAUNCH=1 curl -fsSL https://raw.githubusercontent.com/doingsomethingwithai-commits/better-hyprland-gui/main/scripts/bootstrap.sh | bash
```

## Recovery Commands

If the GUI update button does not work, use these fallback commands:

If you run them inside this repository, they update or delete this checkout directly. Otherwise they fall back to `APP_DIR`.

When the scripts are piped from `curl | bash`, they now ignore unrelated parent git directories and use the installed checkout path instead.

`hard-update.sh` rebuilds the GUI after refreshing the checkout and then launches the rebuilt binary unless you set `NO_LAUNCH=1`. Updates now require a clean checkout and a fast-forwardable branch; commit or stash local changes first.

```bash
curl -fsSL https://raw.githubusercontent.com/doingsomethingwithai-commits/better-hyprland-gui/main/scripts/hard-update.sh | bash
```

To rebuild without launching the app afterwards:

```bash
NO_LAUNCH=1 curl -fsSL https://raw.githubusercontent.com/doingsomethingwithai-commits/better-hyprland-gui/main/scripts/hard-update.sh | bash
```

```bash
APP_REF=v0.1.0 curl -fsSL https://raw.githubusercontent.com/doingsomethingwithai-commits/better-hyprland-gui/main/scripts/hard-update.sh | bash
```

If you need to remove the whole local checkout first:

> [!WARNING]
> This permanently removes the detected Better Hyprland GUI checkout. The script refuses paths outside your home directory and verifies the `hyprgui` Cargo package before deleting anything.

```bash
curl -fsSL https://raw.githubusercontent.com/doingsomethingwithai-commits/better-hyprland-gui/main/scripts/hard-delete.sh | bash
```

## Hyprland Install

Hyprland itself is installed from inside the app:

- Open the GUI.
- Go to the Hyprland install page.
- Click `Install Hyprland`.
- Click `Update Hyprland` when you want the GUI to detect your distro and run the right update command.
- Click `Update Software` when you want the GUI to pull the latest code from GitHub and rebuild itself.
- Use the recovery commands above if the software update path is broken.
- Enter a version or ref in the app if you want to pin a specific repo branch, tag, or commit SHA.
- For Hyprland version pinning, use a NixOS flake ref such as `nixpkgs/release-20.09` or `github:NixOS/nixpkgs/<ref>`.

That keeps the install flow inside the GUI and avoids a separate Hyprland one-liner.

## Dotfiles Install and Activation

Open the `.files` workspace, add a Git repository, and use the profile actions in this order:

1. Click `Install profile` to clone the repository into the configured install path.
2. Select the profile and click `Apply profile` to copy its supported configuration into your home directory.
3. The GUI backs up files it is about to replace under `~/.config/hyprgui/backups/` and records the active profile so it can safely remove files managed by the previous activation.
4. Hyprland is reloaded with `hyprctl reload` when available; otherwise the GUI tells you to reload it manually.

The activation flow recognizes common layouts: a repository root or `dots/`, `home/`, or `dotfiles/` directory that mirrors `$HOME`, an XDG `config/` tree, a `hypr/` directory, and GNU Stow-style packages such as `hyprland/.config/`. Repository symlinks and existing destination symlinks are refused to avoid copying files outside the selected home directory. Sensitive paths such as `.ssh` are intentionally not activated automatically.

## Notes

- Hyprland installation and update flows are officially tested on Arch Linux and NixOS.
- Athena OS, Manjaro, and EndeavourOS use the Arch-style package path.
- Fedora and openSUSE have package-manager integrations, but package availability can vary.
- Ubuntu and Debian can bootstrap the GUI build dependencies, but the GUI does not automatically install Hyprland there.
- The bootstrap script only prepares the system and starts the app.

## Manual Build

1. Install Rust with `rustup` or your distro package manager.
2. Install `git`, `gtk4`, `pango`, Cairo, ATK, `pkg-config`, and a C build toolchain.
3. Clone this repository:

```bash
git clone https://github.com/doingsomethingwithai-commits/better-hyprland-gui
cd better-hyprland-gui
```

4. Build and run:

```bash
cargo build --release
cargo run --release
```

## Suggested Workflow

1. Install the GUI.
2. Open the Hyprland install page and click the install, Hyprland update, or software update button.
3. If the app is broken, run the hard update command from the repo.
4. If you need a clean slate, run the hard delete command from the repo and reinstall.
5. If the app still does not appear in your desktop menu after a reinstall, rerun the bootstrap script so the launcher entry is refreshed.
6. Open the `.files` workspace, add a profile, or use the quick-install form for a GitHub repository URL.
7. Return to the main config pages and tune Hyprland settings.

## Why This Layout

This repository is intentionally split into two layers:

- A GUI for configuration and setup assistance.
- A bootstrap script for system preparation and app startup.

The Hyprland package install now lives in the GUI so there is only one visible install path for Hyprland itself. Dotfile browsing and installation likewise share one `.files` workspace, so users can select a profile, inspect its preview, and install or update it without switching pages.

## TODO

- [x] Implement GUI
- [x] Implement parser
- [x] Add setup pages for dotfiles and Hyprland
