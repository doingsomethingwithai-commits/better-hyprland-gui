<div align="center">

<h1>Better Hyprland GUI</h1>

<p>A GTK4 and Rust helper for configuring Hyprland, onboarding dotfiles, and guiding Linux setup.</p>

![Preview](.github/preview.png)

</div>

## What it does

- Edit Hyprland configuration through a desktop UI.
- Open a dedicated page for installing dotfiles from GitHub repository links.
- Open a dedicated page for Hyprland installation and update guidance on Linux.
- Install and update Hyprland directly from the GUI, and update the software itself from GitHub when needed.

## Quick Install

Use this to install the GUI and its local dependencies. After the build finishes, the script launches the app automatically unless you set `NO_LAUNCH=1`:
It also installs a desktop launcher entry into your user applications folder, so the GUI should appear in your app menu.

```bash
curl -fsSL https://raw.githubusercontent.com/doingsomethingwithai-commits/better-hyprland-gui/main/scripts/bootstrap.sh | bash
```

To pin a specific repository version during install, set `APP_REF` to a branch, tag, or commit SHA:

```bash
APP_REF=v0.1.0 curl -fsSL https://raw.githubusercontent.com/doingsomethingwithai-commits/better-hyprland-gui/main/scripts/bootstrap.sh | bash
```

To skip automatic launch:

```bash
NO_LAUNCH=1 curl -fsSL https://raw.githubusercontent.com/doingsomethingwithai-commits/better-hyprland-gui/main/scripts/bootstrap.sh | bash
```

## Recovery Commands

If the GUI update button does not work, use these fallback commands:

If you run them inside a git checkout, they update or delete that checkout directly. Otherwise they fall back to `APP_DIR`.

`hard-update.sh` rebuilds the GUI after refreshing the checkout and then launches the rebuilt binary unless you set `NO_LAUNCH=1`.

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

## Notes

- Hyprland is officially tested on Arch Linux and NixOS.
- Athena OS is handled as an Arch-like path in the bootstrap script.
- Other Linux distributions may work, but support and package availability can vary.
- The bootstrap script only prepares the system and starts the app.

## Manual Build

1. Install Rust with `rustup` or your distro package manager.
2. Install `git`, `gtk4`, and `pango` development packages.
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
6. Open the dotfiles page and paste a GitHub repository URL.
7. Return to the main config pages and tune Hyprland settings.

## Why This Layout

This repository is intentionally split into two layers:

- A GUI for configuration and setup assistance.
- A bootstrap script for system preparation and app startup.

The Hyprland package install now lives in the GUI so there is only one visible install path for Hyprland itself.

## TODO

- [x] Implement GUI
- [x] Implement parser
- [x] Add setup pages for dotfiles and Hyprland
