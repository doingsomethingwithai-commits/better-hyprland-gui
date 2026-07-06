<div align="center">

<h1>Better Hyprland GUI</h1>

<p>A GTK4 and Rust helper for configuring Hyprland, onboarding dotfiles, and guiding Linux setup.</p>

![Preview](.github/preview.png)

</div>

## What it does

- Edit Hyprland configuration through a desktop UI.
- Open a dedicated page for installing dotfiles from GitHub repository links.
- Open a dedicated page for Hyprland installation and update guidance on Linux.
- Install Hyprland directly from the GUI, not from a separate one-liner installer.

## Quick Install

Use this to install the GUI and its local dependencies:

```bash
curl -fsSL https://raw.githubusercontent.com/doingsomethingwithai-commits/better-hyprland-gui/main/scripts/bootstrap.sh | bash
```

## Hyprland install

Hyprland itself is installed from inside the app:

- Open the GUI.
- Go to the Hyprland install page.
- Click `Install Hyprland`.

That keeps the install flow inside the GUI and avoids a separate Hyprland one-liner.

## Notes

- Hyprland is officially tested on Arch Linux and NixOS.
- Athena OS is handled as an Arch-like path in the bootstrap script.
- Other Linux distributions may work, but support and package availability can vary.
- The bootstrap script only prepares the system and starts the app.

## Manual build

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

## Suggested workflow

1. Install the GUI.
2. Open the Hyprland install page and click the install button.
3. Open the dotfiles page and paste a GitHub repository URL.
4. Return to the main config pages and tune Hyprland settings.

## Why this layout

This repository is intentionally split into two layers:

- A GUI for configuration and setup assistance.
- A bootstrap script for system preparation and app startup.

The Hyprland package install now lives in the GUI so there is only one visible install path for Hyprland itself.

## TODO

- [x] Implement GUI
- [x] Implement parser
- [x] Add setup pages for dotfiles and Hyprland
- [x] Improve the README
- [x] Move Hyprland install into the GUI
- [ ] Improve parser
- [ ] Improve GUI
- [ ] Add more distro-specific installer helpers

## Credits

- Nyx - parser and core GUI work
- Adam Perkowski - base GUI and AUR support
- Vaxry - Hyprland
- gtk-rs - GTK4 bindings
- Hyprland - the window manager

<h6 align="center">Copyright (C) 2024 HyprUtils</h6>
