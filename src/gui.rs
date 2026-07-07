use gtk::{
    gdk, gio, glib, prelude::*, Application, ApplicationWindow, Box, Button, ColorButton,
    DropDown, Entry, Frame, HeaderBar, Image, Label, ListBox, ListBoxRow, MessageDialog,
    Orientation, Popover, ScrolledWindow, Separator, SpinButton, Stack, StackSidebar, StringList,
    Switch, Widget,
};

use hyprparser::HyprlandConfig;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct FileProfile {
    name: String,
    repo_url: String,
    install_path: String,
    version_ref: String,
    notes: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct FileProfileStore {
    profiles: Vec<FileProfile>,
    selected: Option<String>,
}

fn add_dropdown_option(
    container: &Box,
    options: &mut HashMap<String, Widget>,
    name: &str,
    label: &str,
    description: &str,
    items: &[&str],
) {
    let hbox = Box::new(Orientation::Horizontal, 10);
    hbox.set_margin_start(10);
    hbox.set_margin_end(10);
    hbox.set_margin_top(5);
    hbox.set_margin_bottom(5);

    let label_box = Box::new(Orientation::Horizontal, 5);
    label_box.set_hexpand(true);

    let label_widget = Label::new(Some(label));
    label_widget.set_halign(gtk::Align::Start);

    let tooltip_button = Button::new();
    let question_mark_icon = Image::from_icon_name("dialog-question-symbolic");
    tooltip_button.set_child(Some(&question_mark_icon));
    tooltip_button.set_has_frame(false);

    let popover = Popover::new();
    let description_label = Label::new(Some(description));
    description_label.set_margin_top(5);
    description_label.set_margin_bottom(5);
    description_label.set_margin_start(5);
    description_label.set_margin_end(5);
    popover.set_child(Some(&description_label));
    popover.set_position(gtk::PositionType::Right);

    tooltip_button.connect_clicked(move |button| {
        popover.set_parent(button);
        popover.popup();
    });

    label_box.append(&label_widget);
    label_box.append(&tooltip_button);

    let string_list = StringList::new(items);
    let dropdown = DropDown::new(Some(string_list), None::<gtk::Expression>);
    dropdown.set_halign(gtk::Align::End);
    dropdown.set_width_request(100);

    hbox.append(&label_box);
    hbox.append(&dropdown);

    container.append(&hbox);

    options.insert(name.to_string(), dropdown.upcast());
}

fn show_message_dialog(
    parent: &ApplicationWindow,
    message_type: gtk::MessageType,
    title: &str,
    text: &str,
) {
    let dialog = MessageDialog::builder()
        .transient_for(parent)
        .message_type(message_type)
        .buttons(gtk::ButtonsType::Ok)
        .title(title)
        .text(text)
        .modal(true)
        .build();

    dialog.connect_response(|dialog, _| {
        dialog.close();
    });

    dialog.show();
}

fn open_uri(parent: &ApplicationWindow, uri: &str) {
    let trimmed = uri.trim();

    if trimmed.is_empty() {
        show_message_dialog(
            parent,
            gtk::MessageType::Warning,
            "Missing Link",
            "Please paste a GitHub link first.",
        );
        return;
    }

    if let Err(err) = gio::AppInfo::launch_default_for_uri(trimmed, None::<&gio::AppLaunchContext>)
    {
        show_message_dialog(
            parent,
            gtk::MessageType::Error,
            "Could Not Open Link",
            &format!("Failed to open the link: {}", err),
        );
    }
}

fn copy_text_to_clipboard(text: &str) {
    if let Some(display) = gdk::Display::default() {
        display.clipboard().set_text(text);
    }
}

fn distro_id() -> String {
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if let Some(value) = line.strip_prefix("ID=") {
                return value.trim_matches('"').to_lowercase();
            }
        }
    }
    "unknown".to_string()
}

fn show_install_result(parent: &ApplicationWindow, title: &str, success: bool, output: &str) {
    let message_type = if success {
        gtk::MessageType::Info
    } else {
        gtk::MessageType::Error
    };

    show_message_dialog(parent, message_type, title, output);
}

fn find_repo_root(start: PathBuf) -> Option<PathBuf> {
    let mut current = Some(start.as_path());
    while let Some(path) = current {
        if path.join(".git").exists() {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }
    None
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn default_app_dir() -> Option<PathBuf> {
    home_dir().map(|path| path.join(".local").join("share").join("better-hyprland-gui"))
}

fn install_state_path() -> Option<PathBuf> {
    let config_root = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|path| path.join(".config")))?;

    Some(config_root.join("hyprgui").join("install.env"))
}

fn install_state_repo_dirs() -> Vec<PathBuf> {
    let Some(state_path) = install_state_path() else {
        return Vec::new();
    };

    let Ok(contents) = fs::read_to_string(state_path) else {
        return Vec::new();
    };

    contents
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter_map(|(key, value)| match key.trim() {
            "APP_DIR" | "HYPRGUI_REPO_DIR" => {
                let value = value.trim();
                if value.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(value))
                }
            }
            _ => None,
        })
        .collect()
}

fn software_repo_dir() -> Option<PathBuf> {
    let mut candidates = vec![
        std::env::current_dir().ok(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf())),
        default_app_dir(),
    ];

    if let Some(app_dir) = env::var_os("APP_DIR") {
        candidates.push(Some(PathBuf::from(app_dir)));
    }

    if let Some(repo_dir) = env::var_os("HYPRGUI_REPO_DIR") {
        candidates.push(Some(PathBuf::from(repo_dir)));
    }

    candidates.extend(install_state_repo_dirs().into_iter().map(Some));

    for candidate in candidates.into_iter().flatten() {
        if let Some(repo_root) = find_repo_root(candidate) {
            return Some(repo_root);
        }
    }

    None
}

fn entry_text_or_none(entry: &Entry) -> Option<String> {
    let text = entry.text().trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn checkout_repo_ref(repo_dir: &Path, version_ref: &str) -> Result<(), String> {
    let candidates = [
        version_ref.to_string(),
        format!("origin/{version_ref}"),
        format!("refs/tags/{version_ref}"),
    ];

    let mut last_error = String::new();

    for candidate in candidates {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_dir)
            .args(["checkout", "--force", &candidate])
            .output()
            .map_err(|err| format!("Failed to start git checkout for {candidate}: {err}"))?;

        if output.status.success() {
            return Ok(());
        }

        last_error = String::from_utf8_lossy(&output.stderr).to_string();
    }

    Err(format!(
        "Unable to checkout version ref '{version_ref}'. Last git error:\n\n{last_error}"
    ))
}

fn update_repo_checkout(repo_dir: &Path) -> Result<(), String> {
    let current_branch_output = Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_err(|err| format!("Failed to start git branch detection: {err}"))?;

    let current_branch = if current_branch_output.status.success() {
        String::from_utf8_lossy(&current_branch_output.stdout)
            .trim()
            .to_string()
    } else {
        String::new()
    };

    let remote_branch = if !current_branch.is_empty() && current_branch != "HEAD" {
        format!("origin/{current_branch}")
    } else {
        "origin/main".to_string()
    };

    let fetch_output = Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .args(["fetch", "--prune", "--tags", "origin"])
        .output()
        .map_err(|err| format!("Failed to start git fetch: {err}"))?;

    if !fetch_output.status.success() {
        return Err(format!(
            "The git fetch command failed.\n\n{}",
            String::from_utf8_lossy(&fetch_output.stderr)
        ));
    }

    let reset_candidates = [remote_branch, "origin/main".to_string(), "HEAD".to_string()];
    let mut last_error = String::new();

    for candidate in reset_candidates {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_dir)
            .args(["reset", "--hard", &candidate])
            .output()
            .map_err(|err| format!("Failed to start git reset for {candidate}: {err}"))?;

        if output.status.success() {
            return Ok(());
        }

        last_error = String::from_utf8_lossy(&output.stderr).to_string();
    }

    Err(format!(
        "Unable to update the repository checkout. Last git error:\n\n{last_error}"
    ))
}

fn nix_flake_ref_for_hyprland(version_ref: Option<&str>) -> String {
    match version_ref.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value.contains('#') => value.to_string(),
        Some(value) => format!("{value}#hyprland"),
        None => "nixpkgs#hyprland".to_string(),
    }
}

fn run_hyprland_command(
    parent: &ApplicationWindow,
    mut command: Command,
    success_title: &str,
    success_message: &str,
    failure_title: &str,
) {
    match command.output() {
        Ok(output) if output.status.success() => {
            show_install_result(parent, success_title, true, success_message);
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            show_install_result(
                parent,
                failure_title,
                false,
                &format!("The command failed.\n\n{}", stderr),
            );
        }
        Err(err) => {
            show_install_result(
                parent,
                failure_title,
                false,
                &format!("Failed to start the command: {}", err),
            );
        }
    }
}

fn rebuild_software_from_repo(parent: &ApplicationWindow, repo_dir: &Path) {
    let mut cargo_command = Command::new("cargo");
    cargo_command.current_dir(repo_dir).args(["build", "--release"]);

    match cargo_command.output() {
        Ok(output) if output.status.success() => {
            show_install_result(
                parent,
                "Software Updated",
                true,
                "The GUI repository was updated and rebuilt successfully. Restart the application to use the latest version.",
            );
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            show_install_result(
                parent,
                "Software Update Failed",
                false,
                &format!("The rebuild command failed.\n\n{}", stderr),
            );
        }
        Err(err) => {
            show_install_result(
                parent,
                "Software Update Failed",
                false,
                &format!("Failed to start the rebuild command: {}", err),
            );
        }
    }
}

fn update_software_from_github(parent: &ApplicationWindow, version_ref: Option<&str>) {
    let Some(repo_dir) = software_repo_dir() else {
        show_message_dialog(
            parent,
            gtk::MessageType::Warning,
            "Repository Not Found",
            "I could not find the local Git checkout for this GUI. Please open the app from the cloned repository or set APP_DIR/HYPRGUI_REPO_DIR.",
        );
        return;
    };

    let pinned_ref = version_ref.map(str::trim).filter(|value| !value.is_empty());

    if let Some(version_ref) = pinned_ref {
        match update_repo_checkout(&repo_dir) {
            Ok(()) => match checkout_repo_ref(&repo_dir, version_ref) {
                Ok(()) => {
                    rebuild_software_from_repo(parent, &repo_dir);
                }
                Err(err) => {
                    show_install_result(
                        parent,
                        "Software Update Failed",
                        false,
                        &err,
                    );
                }
            },
            Err(err) => {
                show_install_result(
                    parent,
                    "Software Update Failed",
                    false,
                    &err,
                );
            }
        }
        return;
    }

    match update_repo_checkout(&repo_dir) {
        Ok(()) => {
            rebuild_software_from_repo(parent, &repo_dir);
        }
        Err(err) => {
            show_install_result(
                parent,
                "Software Update Failed",
                false,
                &err,
            );
        }
    }
}

fn install_hyprland_from_gui(parent: &ApplicationWindow, version_ref: Option<&str>) {
    let distro = distro_id();
    let command = match distro.as_str() {
        "arch" | "manjaro" | "endeavouros" | "athena" | "athenaos" => {
            let mut command = Command::new("pkexec");
            command.args(["pacman", "-Sy", "--needed", "--noconfirm", "hyprland"]);
            command
        }
        "fedora" => {
            let mut command = Command::new("pkexec");
            command.args(["dnf", "install", "-y", "hyprland"]);
            command
        }
        "opensuse" | "opensuse-tumbleweed" | "suse" => {
            let mut command = Command::new("pkexec");
            command.args(["zypper", "--non-interactive", "install", "hyprland"]);
            command
        }
        "nixos" => {
            let mut command = Command::new("nix");
            command.arg("profile").arg("install").arg(nix_flake_ref_for_hyprland(version_ref));
            command
        }
        _ => {
            show_message_dialog(
                parent,
                gtk::MessageType::Warning,
                "Unsupported Distro",
                "This GUI can only auto-install Hyprland on supported package-manager paths. Use the install guide for manual steps.",
            );
            return;
        }
    };

    run_hyprland_command(
        parent,
        command,
        "Hyprland Installed",
        "Hyprland installation finished successfully. Log out and select the Hyprland session if needed.",
        "Hyprland Install Failed",
    );
}

fn update_hyprland_from_gui(parent: &ApplicationWindow, version_ref: Option<&str>) {
    let distro = distro_id();
    let command = match distro.as_str() {
        "arch" | "manjaro" | "endeavouros" | "athena" | "athenaos" => {
            let mut command = Command::new("pkexec");
            command.args(["pacman", "-Syu", "--needed", "--noconfirm", "hyprland"]);
            command
        }
        "fedora" => {
            let mut command = Command::new("pkexec");
            command.args(["dnf", "upgrade", "-y", "hyprland"]);
            command
        }
        "opensuse" | "opensuse-tumbleweed" | "suse" => {
            let mut command = Command::new("pkexec");
            command.args(["zypper", "--non-interactive", "update", "hyprland"]);
            command
        }
        "nixos" => {
            let mut command = Command::new("nix");
            if version_ref.is_some() {
                command
                    .arg("profile")
                    .arg("install")
                    .arg(nix_flake_ref_for_hyprland(version_ref));
            } else {
                command.args(["profile", "upgrade", "--regex", ".*hyprland.*"]);
            }
            command
        }
        _ => {
            show_message_dialog(
                parent,
                gtk::MessageType::Warning,
                "Unsupported Distro",
                "This GUI can only auto-update Hyprland on supported package-manager paths. Use the update guide for manual steps.",
            );
            return;
        }
    };

    run_hyprland_command(
        parent,
        command,
        "Hyprland Updated",
        "Hyprland update finished successfully. Restart or log out if the new version requires it.",
        "Hyprland Update Failed",
    );
}

fn spotlight_state_path() -> PathBuf {
    Path::new(&env::var("HOME").unwrap_or_else(|_| ".".to_string()))
        .join(".config")
        .join("hyprgui")
        .join("spotlight_seen")
}

pub fn should_show_spotlight_guide() -> bool {
    !spotlight_state_path().exists()
}

fn mark_spotlight_guide_seen() {
    let path = spotlight_state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, "seen");
}

fn file_profiles_state_path() -> PathBuf {
    Path::new(&env::var("HOME").unwrap_or_else(|_| ".".to_string()))
        .join(".config")
        .join("hyprgui")
        .join("file_profiles.json")
}

fn load_file_profile_store() -> FileProfileStore {
    let path = file_profiles_state_path();
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => FileProfileStore::default(),
    }
}

fn save_file_profile_store(store: &FileProfileStore) {
    let path = file_profiles_state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(content) = serde_json::to_string_pretty(store) {
        let _ = fs::write(path, content);
    }
}

fn default_file_install_path() -> String {
    Path::new(&env::var("HOME").unwrap_or_else(|_| ".".to_string()))
        .join("dotfiles")
        .to_string_lossy()
        .to_string()
}

fn file_profile_clone_command(profile: &FileProfile) -> String {
    let install_path = if profile.install_path.trim().is_empty() {
        default_file_install_path()
    } else {
        profile.install_path.trim().to_string()
    };

    let repo_url = profile.repo_url.trim();
    let version_ref = profile.version_ref.trim();

    if version_ref.is_empty() {
        format!("git clone {} {}", repo_url, install_path)
    } else {
        format!("git clone --branch {} {} {}", version_ref, repo_url, install_path)
    }
}

fn install_file_profile(parent: &ApplicationWindow, profile: &FileProfile) {
    let repo_url = profile.repo_url.trim();
    if repo_url.is_empty() {
        show_message_dialog(
            parent,
            gtk::MessageType::Warning,
            "Missing Repo",
            "The selected .file profile does not contain a GitHub repository URL.",
        );
        return;
    }

    let install_path = if profile.install_path.trim().is_empty() {
        default_file_install_path()
    } else {
        profile.install_path.trim().to_string()
    };

    let target_path = PathBuf::from(&install_path);
    if target_path.exists() {
        if !target_path.join(".git").exists() {
            show_message_dialog(
                parent,
                gtk::MessageType::Warning,
                "Existing Folder",
                "The install path already exists, but it is not a Git repository. Pick a different path or remove the folder first.",
            );
            return;
        }

        let mut command = Command::new("git");
        command.arg("-C").arg(&target_path).args(["pull", "--rebase"]);

        match command.output() {
            Ok(output) if output.status.success() => {
                if profile.version_ref.trim().is_empty() {
                    show_install_result(
                        parent,
                        "Dotfiles Updated",
                        true,
                        "The selected .file profile was updated successfully.",
                    );
                } else {
                    let mut fetch_command = Command::new("git");
                    fetch_command
                        .arg("-C")
                        .arg(&target_path)
                        .args(["fetch", "--tags", "origin"]);

                    match fetch_command.output() {
                        Ok(fetch_output) if fetch_output.status.success() => {
                            match checkout_repo_ref(&target_path, profile.version_ref.trim()) {
                                Ok(()) => show_install_result(
                                    parent,
                                    "Dotfiles Updated",
                                    true,
                                    "The selected .file profile was updated and switched to the requested version.",
                                ),
                                Err(err) => show_install_result(
                                    parent,
                                    "Dotfiles Update Failed",
                                    false,
                                    &err,
                                ),
                            }
                        }
                        Ok(fetch_output) => {
                            let stderr = String::from_utf8_lossy(&fetch_output.stderr);
                            show_install_result(
                                parent,
                                "Dotfiles Update Failed",
                                false,
                                &format!("The git fetch command failed.\n\n{}", stderr),
                            );
                        }
                        Err(err) => {
                            show_install_result(
                                parent,
                                "Dotfiles Update Failed",
                                false,
                                &format!("Failed to start the git fetch command: {}", err),
                            );
                        }
                    }
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                show_install_result(
                    parent,
                    "Dotfiles Update Failed",
                    false,
                    &format!("The git pull command failed.\n\n{}", stderr),
                );
            }
            Err(err) => {
                show_install_result(
                    parent,
                    "Dotfiles Update Failed",
                    false,
                    &format!("Failed to start the git pull command: {}", err),
                );
            }
        }

        return;
    }

    if let Some(parent_dir) = target_path.parent() {
        if let Err(err) = fs::create_dir_all(parent_dir) {
            show_message_dialog(
                parent,
                gtk::MessageType::Error,
                "Could Not Prepare Path",
                &format!("Failed to create the parent folder: {}", err),
            );
            return;
        }
    }

    let mut command = Command::new("git");
    command.arg("clone");
    if !profile.version_ref.trim().is_empty() {
        command.args(["--branch", profile.version_ref.trim()]);
    }
    command.arg(repo_url).arg(&install_path);

    run_hyprland_command(
        parent,
        command,
        "Dotfiles Installed",
        "The selected .file profile was installed successfully.",
        "Dotfiles Install Failed",
    );
}

fn clear_listbox(list_box: &ListBox) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
}

fn button_with_icon_label(icon_name: &str, text: &str) -> Button {
    let button = Button::new();
    let inner = Box::new(Orientation::Horizontal, 6);
    let icon = Image::from_icon_name(icon_name);
    let label = Label::new(Some(text));
    inner.append(&icon);
    inner.append(&label);
    button.set_child(Some(&inner));
    button
}

#[derive(Clone)]
struct SpotlightStep {
    title: &'static str,
    target: &'static str,
    body: &'static str,
    tip: &'static str,
}

fn spotlight_steps() -> Vec<SpotlightStep> {
    vec![
        SpotlightStep {
            title: "Welcome",
            target: "Main navigation",
            body: "This guide gives you a quick tour of the most important controls. You can skip it now and reopen it later from the gear menu.",
            tip: "Use Next to move through the tour.",
        },
        SpotlightStep {
            title: "Save",
            target: "Save button",
            body: "The Save button writes the current Hyprland configuration changes back to your config file.",
            tip: "Use this after editing values in the configuration pages.",
        },
        SpotlightStep {
            title: "Settings",
            target: "Gear menu",
            body: "The gear menu contains config import/export actions and the Spotlight Guide entry.",
            tip: "Use it when you want to reload a GUI profile or reopen this guide.",
        },
        SpotlightStep {
            title: "Dotfiles",
            target: "Dotfiles page",
            body: "Paste a GitHub link for your dotfiles, open it directly, or copy a clone command to start the setup flow.",
            tip: "This is the easiest way to bootstrap a config repository.",
        },
        SpotlightStep {
            title: "Install",
            target: "Hyprland install page",
            body: "Use the install page for Hyprland installation help, the update guide, and links to the official setup docs.",
            tip: "This is the best place to start on a fresh system.",
        },
    ]
}

fn open_spotlight_guide(parent: &ApplicationWindow) {
    let steps = Rc::new(spotlight_steps());
    let current_index = Rc::new(RefCell::new(0usize));

    let guide_window = ApplicationWindow::builder()
        .transient_for(parent)
        .modal(true)
        .title("Spotlight Guide")
        .default_width(760)
        .default_height(420)
        .build();

    let root = Box::new(Orientation::Vertical, 16);
    root.set_margin_top(18);
    root.set_margin_bottom(18);
    root.set_margin_start(18);
    root.set_margin_end(18);

    let title_label = Label::new(None);
    title_label.set_markup("<b>Spotlight Guide</b>");
    title_label.set_halign(gtk::Align::Start);

    let step_label = Label::new(None);
    step_label.set_halign(gtk::Align::Start);
    step_label.set_wrap(true);

    let target_frame = Frame::new(None);
    let target_label = Label::new(None);
    target_label.set_margin_top(18);
    target_label.set_margin_bottom(18);
    target_label.set_margin_start(18);
    target_label.set_margin_end(18);
    target_label.set_wrap(true);
    target_label.set_halign(gtk::Align::Start);
    target_frame.set_child(Some(&target_label));

    let body_label = Label::new(None);
    body_label.set_wrap(true);
    body_label.set_halign(gtk::Align::Start);

    let tip_label = Label::new(None);
    tip_label.set_wrap(true);
    tip_label.set_halign(gtk::Align::Start);
    tip_label.set_opacity(0.75);

    let button_row = Box::new(Orientation::Horizontal, 10);
    let back_button = Button::with_label("Back");
    let next_button = Button::with_label("Next");
    let skip_button = Button::with_label("Skip Guide");
    let finish_button = Button::with_label("Finish");

    button_row.append(&back_button);
    button_row.append(&next_button);
    button_row.append(&skip_button);
    button_row.append(&finish_button);

    root.append(&title_label);
    root.append(&step_label);
    root.append(&target_frame);
    root.append(&body_label);
    root.append(&tip_label);
    root.append(&button_row);

    guide_window.set_child(Some(&root));

    let window_for_skip = guide_window.clone();
    skip_button.connect_clicked(move |_| {
        mark_spotlight_guide_seen();
        window_for_skip.close();
    });

    let window_for_finish = guide_window.clone();
    finish_button.connect_clicked(move |_| {
        mark_spotlight_guide_seen();
        window_for_finish.close();
    });

    let step_label_back = step_label.clone();
    let target_label_back = target_label.clone();
    let body_label_back = body_label.clone();
    let tip_label_back = tip_label.clone();
    let next_button_back = next_button.clone();
    let back_button_back = back_button.clone();
    let current_index_back = current_index.clone();
    let steps_back = steps.clone();
    let update_step = Rc::new(move || {
        let index = *current_index_back.borrow();
        let step = &steps_back[index];
        step_label_back.set_markup(&format!("<span size=\"large\"><b>{}</b></span>", step.title));
        target_label_back.set_markup(&format!("<b>Spotlight:</b> {}", step.target));
        body_label_back.set_text(step.body);
        tip_label_back.set_text(step.tip);
        back_button_back.set_sensitive(index > 0);
        next_button_back.set_sensitive(true);
        next_button_back.set_label(if index + 1 < steps_back.len() { "Next" } else { "Finish" });
    });

    let update_step_back = update_step.clone();
    let current_index_prev = current_index.clone();
    back_button.connect_clicked(move |_| {
        let mut index = current_index_prev.borrow_mut();
        if *index > 0 {
            *index -= 1;
        }
        update_step_back();
    });

    let update_step_next = update_step.clone();
    let current_index_next = current_index.clone();
    let steps_next = steps.clone();
    let window_for_next = guide_window.clone();
    next_button.connect_clicked(move |_| {
        let mut index = current_index_next.borrow_mut();
        if *index + 1 < steps_next.len() {
            *index += 1;
            update_step_next();
        } else {
            mark_spotlight_guide_seen();
            window_for_next.close();
        }
    });

    update_step();
    guide_window.present();
}

pub struct ConfigGUI {
    pub window: ApplicationWindow,
    config_widgets: HashMap<String, ConfigWidget>,
    pub save_button: Button,
    content_box: Box,
    changed_options: Rc<RefCell<HashMap<(String, String), String>>>,
    file_profiles: Rc<RefCell<FileProfileStore>>,
    file_profiles_refresh: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    stack: Stack,
    sidebar: StackSidebar,
    load_config_button: Button,
    save_config_button: Button,
    pub gear_menu: Rc<RefCell<Popover>>,
}

impl ConfigGUI {
    pub fn new(app: &Application) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .default_width(1000)
            .default_height(600)
            .build();

        let header_bar = HeaderBar::builder()
            .show_title_buttons(false)
            .title_widget(&gtk::Label::new(Some("Hyprland Configuration")))
            .build();

        let gear_button = Button::from_icon_name("emblem-system-symbolic");
        header_bar.pack_start(&gear_button);

        let gear_menu = Rc::new(RefCell::new(Popover::new()));
        gear_menu.borrow().set_parent(&gear_button);

        let gear_menu_box = Box::new(Orientation::Vertical, 5);
        gear_menu_box.set_margin_top(5);
        gear_menu_box.set_margin_bottom(5);
        gear_menu_box.set_margin_start(5);
        gear_menu_box.set_margin_end(5);

        let save_config_button = Button::with_label("Save HyprGUI Config");
        let load_config_button = Button::with_label("Load HyprGUI Config");
        let spotlight_guide_button = Button::with_label("Spotlight Guide");

        gear_menu_box.append(&load_config_button);
        gear_menu_box.append(&save_config_button);
        gear_menu_box.append(&spotlight_guide_button);

        gear_menu.borrow().set_child(Some(&gear_menu_box));

        let gear_menu_clone = gear_menu.clone();
        gear_button.connect_clicked(move |_| {
            gear_menu_clone.borrow().popup();
        });

        let tooltip_button = Button::new();
        let question_mark_icon = Image::from_icon_name("dialog-question-symbolic");
        tooltip_button.set_child(Some(&question_mark_icon));
        tooltip_button.set_has_frame(false);
        header_bar.pack_start(&tooltip_button);

        let popover = Popover::new();
        let tooltip_text = "The save button saves the options that you chose in the gui and exports it to json format, likewise the load button loads these saved options from the exported json file; automatically filling in the options in the gui with the specified ones in the json file, clicking save to apply these changes is still necessary though.";
        let tooltip_label = Label::new(Some(tooltip_text));
        tooltip_label.set_margin_top(5);
        tooltip_label.set_margin_bottom(5);
        tooltip_label.set_margin_start(5);
        tooltip_label.set_margin_end(5);
        tooltip_label.set_wrap(true);
        tooltip_label.set_max_width_chars(50);
        popover.set_child(Some(&tooltip_label));

        tooltip_button.connect_clicked(move |button| {
            popover.set_parent(button);
            popover.popup();
        });

        let parent = window.clone();
        spotlight_guide_button.connect_clicked(move |_| {
            open_spotlight_guide(&parent);
        });

        let save_button = Button::with_label("Save");
        header_bar.pack_end(&save_button);

        window.set_titlebar(Some(&header_bar));

        let main_box = Box::new(Orientation::Vertical, 0);

        let content_box = Box::new(Orientation::Horizontal, 0);
        main_box.append(&content_box);

        window.set_child(Some(&main_box));

        let config_widgets = HashMap::new();

        let stack = Stack::new();

        let sidebar = StackSidebar::new();
        sidebar.set_stack(&stack);
        sidebar.set_width_request(200);

        ConfigGUI {
            window,
            config_widgets,
            save_button,
            content_box,
            changed_options: Rc::new(RefCell::new(HashMap::new())),
            file_profiles: Rc::new(RefCell::new(load_file_profile_store())),
            file_profiles_refresh: Rc::new(RefCell::new(None)),
            stack,
            sidebar,
            load_config_button,
            save_config_button,
            gear_menu,
        }
    }

    pub fn show_spotlight_guide(&self) {
        open_spotlight_guide(&self.window);
    }

    fn rebuild_navigation(&mut self) {
        while let Some(child) = self.stack.first_child() {
            self.stack.remove(&child);
        }

        while let Some(child) = self.content_box.first_child() {
            self.content_box.remove(&child);
        }

        self.sidebar = StackSidebar::new();
        self.sidebar.set_stack(&self.stack);
        self.sidebar.set_width_request(200);

        self.content_box.append(&self.sidebar);
        self.content_box.append(&self.stack);

        self.stack.connect_visible_child_notify(move |stack| {
            if let Some(child) = stack.visible_child() {
                if let Some(scrolled_window) = child.downcast_ref::<ScrolledWindow>() {
                    let adj = scrolled_window.vadjustment();
                    adj.set_value(adj.lower());
                }
            }
        });
    }

    fn add_setup_overview_page(&mut self, note: &str) {
        let scrolled_window = ScrolledWindow::new();
        scrolled_window.set_vexpand(true);
        scrolled_window.set_hexpand(true);

        let container = Box::new(Orientation::Vertical, 14);
        container.set_margin_top(16);
        container.set_margin_bottom(16);
        container.set_margin_start(16);
        container.set_margin_end(16);

        let title_label = Label::new(Some("Setup Center"));
        title_label.set_markup("<b>Setup Center</b>");
        title_label.set_halign(gtk::Align::Start);

        let note_label = Label::new(Some(note));
        note_label.set_wrap(true);
        note_label.set_halign(gtk::Align::Start);

        let help_label = Label::new(Some(
            "Use the pages in the sidebar to install Hyprland or prepare dotfiles from GitHub links.",
        ));
        help_label.set_wrap(true);
        help_label.set_opacity(0.8);
        help_label.set_halign(gtk::Align::Start);

        container.append(&title_label);
        container.append(&note_label);
        container.append(&help_label);

        scrolled_window.set_child(Some(&container));
        self.stack
            .add_titled(&scrolled_window, Some("setup"), "Setup");
    }

    fn add_files_page(&mut self) {
        let scrolled_window = ScrolledWindow::new();
        scrolled_window.set_vexpand(true);
        scrolled_window.set_hexpand(true);

        let container = Box::new(Orientation::Vertical, 14);
        container.set_margin_top(16);
        container.set_margin_bottom(16);
        container.set_margin_start(16);
        container.set_margin_end(16);

        let title_label = Label::new(Some(".files Library"));
        title_label.set_markup("<b>.files Library</b>");
        title_label.set_halign(gtk::Align::Start);

        let description_label = Label::new(Some(
            "Select a saved .file profile on the left to preview it on the right. Use the plus button to add GitHub repos or local install targets.",
        ));
        description_label.set_wrap(true);
        description_label.set_halign(gtk::Align::Start);
        description_label.set_opacity(0.8);

        let action_row = Box::new(Orientation::Horizontal, 10);
        let add_profile_button = button_with_icon_label("list-add-symbolic", ".file hinzufügen");
        let open_profile_button = Button::with_label("Open Selected Repo");
        let install_profile_button = Button::with_label("Install / Update Selected");
        let copy_command_button = Button::with_label("Copy Clone Command");
        let remove_profile_button = Button::with_label("Remove Selected");

        let body_row = Box::new(Orientation::Horizontal, 14);
        body_row.set_hexpand(true);
        body_row.set_vexpand(true);

        let list_frame = Frame::new(Some("Saved .files"));
        let list_scroller = ScrolledWindow::new();
        list_scroller.set_vexpand(true);
        list_scroller.set_hexpand(false);
        list_scroller.set_min_content_width(300);

        let list_box = ListBox::new();
        list_box.set_vexpand(true);
        list_box.set_selection_mode(gtk::SelectionMode::Single);
        list_scroller.set_child(Some(&list_box));
        list_frame.set_child(Some(&list_scroller));

        let preview_frame = Frame::new(Some("Preview"));
        let preview_box = Box::new(Orientation::Vertical, 10);
        preview_box.set_margin_top(14);
        preview_box.set_margin_bottom(14);
        preview_box.set_margin_start(14);
        preview_box.set_margin_end(14);

        let preview_title = Label::new(Some("No .file selected"));
        preview_title.set_markup("<b>No .file selected</b>");
        preview_title.set_halign(gtk::Align::Start);

        let preview_repo = Label::new(Some("Repo: -"));
        preview_repo.set_halign(gtk::Align::Start);
        preview_repo.set_wrap(true);

        let preview_path = Label::new(Some("Install path: -"));
        preview_path.set_halign(gtk::Align::Start);
        preview_path.set_wrap(true);

        let preview_version = Label::new(Some("Version: latest"));
        preview_version.set_halign(gtk::Align::Start);
        preview_version.set_wrap(true);

        let preview_command = Label::new(Some("Command: -"));
        preview_command.set_halign(gtk::Align::Start);
        preview_command.set_wrap(true);
        preview_command.set_selectable(true);

        let preview_notes = Label::new(Some(
            "Pick a profile to see its clone command and install target.",
        ));
        preview_notes.set_halign(gtk::Align::Start);
        preview_notes.set_wrap(true);
        preview_notes.set_opacity(0.8);

        preview_box.append(&preview_title);
        preview_box.append(&preview_repo);
        preview_box.append(&preview_path);
        preview_box.append(&preview_version);
        preview_box.append(&preview_command);
        preview_box.append(&preview_notes);
        preview_frame.set_child(Some(&preview_box));

        body_row.append(&list_frame);
        body_row.append(&preview_frame);

        let hint_label = Label::new(Some(
            "Wolkup-style flow: choose a profile, inspect the preview, then install or switch to another profile with one click.",
        ));
        hint_label.set_wrap(true);
        hint_label.set_halign(gtk::Align::Start);
        hint_label.set_opacity(0.75);

        let store_for_refresh = self.file_profiles.clone();
        let list_box_for_refresh = list_box.clone();
        let preview_title_for_refresh = preview_title.clone();
        let preview_repo_for_refresh = preview_repo.clone();
        let preview_path_for_refresh = preview_path.clone();
        let preview_version_for_refresh = preview_version.clone();
        let preview_command_for_refresh = preview_command.clone();
        let preview_notes_for_refresh = preview_notes.clone();
        let open_profile_button_for_refresh = open_profile_button.clone();
        let install_profile_button_for_refresh = install_profile_button.clone();
        let copy_command_button_for_refresh = copy_command_button.clone();
        let remove_profile_button_for_refresh = remove_profile_button.clone();
        let initial_selected_name = self.file_profiles.borrow().selected.clone();
        let selected_name_for_refresh = Rc::new(RefCell::new(initial_selected_name));
        let selected_name_for_refresh_clone = selected_name_for_refresh.clone();

        let refresh_ui: Rc<dyn Fn()> = Rc::new(move || {
            let (profiles, selected_name) = {
                let store = store_for_refresh.borrow();
                (store.profiles.clone(), store.selected.clone())
            };

            clear_listbox(&list_box_for_refresh);

            for profile in &profiles {
                let row = ListBoxRow::new();
                row.set_activatable(true);

                let row_box = Box::new(Orientation::Vertical, 3);
                row_box.set_margin_top(10);
                row_box.set_margin_bottom(10);
                row_box.set_margin_start(10);
                row_box.set_margin_end(10);

                let escaped_name = glib::markup_escape_text(&profile.name);
                let name_label = Label::new(Some(&profile.name));
                name_label.set_halign(gtk::Align::Start);
                name_label.set_markup(&format!("<b>{}</b>", escaped_name));

                let path_value = if profile.install_path.trim().is_empty() {
                    default_file_install_path()
                } else {
                    profile.install_path.clone()
                };
                let path_label = Label::new(Some(&path_value));
                path_label.set_halign(gtk::Align::Start);
                path_label.set_wrap(true);
                path_label.set_opacity(0.75);

                row_box.append(&name_label);
                row_box.append(&path_label);
                row.set_child(Some(&row_box));
                list_box_for_refresh.append(&row);
            }

            let selected_name = selected_name
                .or_else(|| selected_name_for_refresh_clone.borrow().clone());
            let selected_index = selected_name.and_then(|wanted| {
                profiles
                    .iter()
                    .position(|profile| profile.name == wanted)
                    .or_else(|| if profiles.is_empty() { None } else { Some(0) })
            });

            if let Some(index) = selected_index {
                if let Some(row) = list_box_for_refresh.row_at_index(index as i32) {
                    list_box_for_refresh.select_row(Some(&row));
                }
            } else if !profiles.is_empty() {
                if let Some(row) = list_box_for_refresh.row_at_index(0) {
                    list_box_for_refresh.select_row(Some(&row));
                }
            } else {
                preview_title_for_refresh.set_markup("<b>No .file selected</b>");
                preview_repo_for_refresh.set_text("Repo: -");
                preview_path_for_refresh.set_text("Install path: -");
                preview_version_for_refresh.set_text("Version: latest");
                preview_command_for_refresh.set_text("Command: -");
                preview_notes_for_refresh.set_text("Pick a profile to see its clone command and install target.");
            }

            let has_profile = !profiles.is_empty();
            open_profile_button_for_refresh.set_sensitive(has_profile);
            install_profile_button_for_refresh.set_sensitive(has_profile);
            copy_command_button_for_refresh.set_sensitive(has_profile);
            remove_profile_button_for_refresh.set_sensitive(has_profile);
        });

        *self.file_profiles_refresh.borrow_mut() = Some(refresh_ui.clone());

        let store_for_selection = self.file_profiles.clone();
        let selected_name_for_selection = selected_name_for_refresh.clone();
        let preview_title_for_selection = preview_title.clone();
        let preview_repo_for_selection = preview_repo.clone();
        let preview_path_for_selection = preview_path.clone();
        let preview_version_for_selection = preview_version.clone();
        let preview_command_for_selection = preview_command.clone();
        let preview_notes_for_selection = preview_notes.clone();
        list_box.connect_row_selected(move |_, row| {
            let Some(row) = row else {
                return;
            };

            let index = row.index();
            if index < 0 {
                return;
            }

            let profile = {
                let store = store_for_selection.borrow();
                store.profiles.get(index as usize).cloned()
            };
            let Some(profile) = profile else {
                return;
            };

            {
                let mut store = store_for_selection.borrow_mut();
                store.selected = Some(profile.name.clone());
                save_file_profile_store(&store);
            }
            *selected_name_for_selection.borrow_mut() = Some(profile.name.clone());
            let escaped_name = glib::markup_escape_text(&profile.name);
            preview_title_for_selection.set_markup(&format!("<b>{}</b>", escaped_name));
            preview_repo_for_selection.set_text(&format!("Repo: {}", profile.repo_url));
            let install_path = if profile.install_path.trim().is_empty() {
                default_file_install_path()
            } else {
                profile.install_path.clone()
            };
            preview_path_for_selection.set_text(&format!("Install path: {}", install_path));
            let version_text = if profile.version_ref.trim().is_empty() {
                "Version: latest".to_string()
            } else {
                format!("Version: {}", profile.version_ref)
            };
            preview_version_for_selection.set_text(&version_text);
            preview_command_for_selection.set_text(&file_profile_clone_command(&profile));
            let notes = if profile.notes.trim().is_empty() {
                "No extra notes were saved for this profile.".to_string()
            } else {
                profile.notes.clone()
            };
            preview_notes_for_selection.set_text(&notes);
        });

        let parent = self.window.clone();
        let store_for_add = self.file_profiles.clone();
        let refresh_ui_for_add = refresh_ui.clone();
        let selected_name_for_add = selected_name_for_refresh.clone();
        add_profile_button.connect_clicked(move |_| {
            let parent = parent.clone();
            let store_for_add = store_for_add.clone();
            let refresh_ui_for_add = refresh_ui_for_add.clone();
            let selected_name_for_add = selected_name_for_add.clone();
            glib::MainContext::default().spawn_local(async move {
                let dialog = gtk::Dialog::with_buttons(
                    Some("Add .file Profile"),
                    Some(&parent),
                    gtk::DialogFlags::MODAL,
                    &[
                        ("Cancel", gtk::ResponseType::Cancel),
                        ("Add", gtk::ResponseType::Accept),
                    ],
                );

                let content = dialog.content_area();
                content.set_spacing(10);
                content.set_margin_top(12);
                content.set_margin_bottom(12);
                content.set_margin_start(12);
                content.set_margin_end(12);

                let name_entry = Entry::new();
                name_entry.set_placeholder_text(Some("Profile name"));
                let repo_entry = Entry::new();
                repo_entry.set_placeholder_text(Some("https://github.com/username/dotfiles"));
                let path_entry = Entry::new();
                path_entry.set_placeholder_text(Some(&default_file_install_path()));
                let version_entry = Entry::new();
                version_entry.set_placeholder_text(Some("Optional branch or tag"));
                let notes_entry = Entry::new();
                notes_entry.set_placeholder_text(Some("Optional notes or config label"));

                content.append(&Label::new(Some("Name")));
                content.append(&name_entry);
                content.append(&Label::new(Some("GitHub repo URL")));
                content.append(&repo_entry);
                content.append(&Label::new(Some("Install path")));
                content.append(&path_entry);
                content.append(&Label::new(Some("Git ref / branch")));
                content.append(&version_entry);
                content.append(&Label::new(Some("Notes")));
                content.append(&notes_entry);

                let response = dialog.run_future().await;
                if response == gtk::ResponseType::Accept {
                    let name = name_entry.text().trim().to_string();
                    let repo_url = repo_entry.text().trim().to_string();
                    let install_path = path_entry.text().trim().to_string();
                    let version_ref = version_entry.text().trim().to_string();
                    let notes = notes_entry.text().trim().to_string();

                    if name.is_empty() || repo_url.is_empty() {
                        show_message_dialog(
                            &parent,
                            gtk::MessageType::Warning,
                            "Missing Data",
                            "Both a profile name and a GitHub repo URL are required.",
                        );
                    } else {
                        {
                            let mut store = store_for_add.borrow_mut();
                            store.profiles.push(FileProfile {
                                name: name.clone(),
                                repo_url,
                                install_path,
                                version_ref,
                                notes,
                            });
                            store.selected = Some(name.clone());
                            save_file_profile_store(&store);
                        }

                        *selected_name_for_add.borrow_mut() = Some(name);
                        refresh_ui_for_add();
                    }
                }

                dialog.close();
            });
        });

        let parent = self.window.clone();
        let store_for_open = self.file_profiles.clone();
        let selected_name_for_open = selected_name_for_refresh.clone();
        let stack_for_open = self.stack.clone();
        open_profile_button.connect_clicked(move |_| {
            let target = selected_name_for_open.borrow().clone();
            if target.is_some() {
                stack_for_open.set_visible_child_name("files");
            }

            let store = store_for_open.borrow();
            if let Some(profile) = target.and_then(|wanted| {
                store.profiles.iter().find(|profile| profile.name == wanted).cloned()
            }) {
                open_uri(&parent, &profile.repo_url);
            }
        });

        let parent = self.window.clone();
        let store_for_copy = self.file_profiles.clone();
        let selected_name_for_copy = selected_name_for_refresh.clone();
        copy_command_button.connect_clicked(move |_| {
            let store = store_for_copy.borrow();
            if let Some(profile) = selected_name_for_copy
                .borrow()
                .as_ref()
                .and_then(|wanted| store.profiles.iter().find(|profile| &profile.name == wanted))
            {
                copy_text_to_clipboard(&file_profile_clone_command(profile));
                show_message_dialog(
                    &parent,
                    gtk::MessageType::Info,
                    "Copied",
                    "The clone command for the selected profile has been copied to the clipboard.",
                );
            }
        });

        let parent = self.window.clone();
        let store_for_install = self.file_profiles.clone();
        let selected_name_for_install = selected_name_for_refresh.clone();
        install_profile_button.connect_clicked(move |_| {
            let store = store_for_install.borrow();
            if let Some(profile) = selected_name_for_install
                .borrow()
                .as_ref()
                .and_then(|wanted| store.profiles.iter().find(|profile| &profile.name == wanted))
            {
                install_file_profile(&parent, profile);
            }
        });

        let parent = self.window.clone();
        let store_for_remove = self.file_profiles.clone();
        let refresh_ui_for_remove = refresh_ui.clone();
        let selected_name_for_remove = selected_name_for_refresh.clone();
        remove_profile_button.connect_clicked(move |_| {
            let selected = selected_name_for_remove.borrow().clone();
            if let Some(selected) = selected {
                let mut store = store_for_remove.borrow_mut();
                store.profiles.retain(|profile| profile.name != selected);
                if store.selected.as_deref() == Some(selected.as_str()) {
                    store.selected = store.profiles.first().map(|profile| profile.name.clone());
                }
                save_file_profile_store(&store);
                *selected_name_for_remove.borrow_mut() = store.selected.clone();
                refresh_ui_for_remove();
            } else {
                show_message_dialog(
                    &parent,
                    gtk::MessageType::Warning,
                    "Nothing Selected",
                    "Select a .file profile first.",
                );
            }
        });

        let button_row = Box::new(Orientation::Horizontal, 10);
        button_row.append(&add_profile_button);
        button_row.append(&open_profile_button);
        button_row.append(&install_profile_button);
        button_row.append(&copy_command_button);
        button_row.append(&remove_profile_button);

        container.append(&title_label);
        container.append(&description_label);
        container.append(&button_row);
        container.append(&Separator::new(Orientation::Horizontal));
        container.append(&body_row);
        container.append(&hint_label);

        scrolled_window.set_child(Some(&container));
        self.stack
            .add_titled(&scrolled_window, Some("files"), ".files");

        refresh_ui();
    }

    fn add_dotfiles_page(&mut self) {
        let scrolled_window = ScrolledWindow::new();
        scrolled_window.set_vexpand(true);
        scrolled_window.set_hexpand(true);

        let container = Box::new(Orientation::Vertical, 14);
        container.set_margin_top(16);
        container.set_margin_bottom(16);
        container.set_margin_start(16);
        container.set_margin_end(16);

        let title_label = Label::new(Some("Dotfiles from GitHub"));
        title_label.set_markup("<b>Dotfiles from GitHub</b>");
        title_label.set_halign(gtk::Align::Start);

        let description_label = Label::new(Some(
            "Paste a GitHub repository link for your dotfiles, then open the repo or copy a starter clone command.",
        ));
        description_label.set_wrap(true);
        description_label.set_halign(gtk::Align::Start);
        description_label.set_opacity(0.8);

        let files_button_row = Box::new(Orientation::Horizontal, 10);
        let open_files_button = Button::with_label("Install .files");
        let add_file_button = button_with_icon_label("list-add-symbolic", ".file hinzufügen");

        let stack_for_open_files = self.stack.clone();
        open_files_button.connect_clicked(move |_| {
            stack_for_open_files.set_visible_child_name("files");
        });

        let parent = self.window.clone();
        let store_for_add = self.file_profiles.clone();
        let refresh_holder_for_add = self.file_profiles_refresh.clone();
        let stack_for_add = self.stack.clone();
        add_file_button.connect_clicked(move |_| {
            let parent = parent.clone();
            let store_for_add = store_for_add.clone();
            let refresh_holder_for_add = refresh_holder_for_add.clone();
            let stack_for_add = stack_for_add.clone();
            glib::MainContext::default().spawn_local(async move {
                let dialog = gtk::Dialog::with_buttons(
                    Some("Add .file Profile"),
                    Some(&parent),
                    gtk::DialogFlags::MODAL,
                    &[
                        ("Cancel", gtk::ResponseType::Cancel),
                        ("Add", gtk::ResponseType::Accept),
                    ],
                );

                let content = dialog.content_area();
                content.set_spacing(10);
                content.set_margin_top(12);
                content.set_margin_bottom(12);
                content.set_margin_start(12);
                content.set_margin_end(12);

                let name_entry = Entry::new();
                name_entry.set_placeholder_text(Some("Profile name"));
                let repo_entry = Entry::new();
                repo_entry.set_placeholder_text(Some("https://github.com/username/dotfiles"));
                let path_entry = Entry::new();
                path_entry.set_placeholder_text(Some(&default_file_install_path()));
                let version_entry = Entry::new();
                version_entry.set_placeholder_text(Some("Optional branch or tag"));
                let notes_entry = Entry::new();
                notes_entry.set_placeholder_text(Some("Optional notes or config label"));

                content.append(&Label::new(Some("Name")));
                content.append(&name_entry);
                content.append(&Label::new(Some("GitHub repo URL")));
                content.append(&repo_entry);
                content.append(&Label::new(Some("Install path")));
                content.append(&path_entry);
                content.append(&Label::new(Some("Git ref / branch")));
                content.append(&version_entry);
                content.append(&Label::new(Some("Notes")));
                content.append(&notes_entry);

                if dialog.run_future().await == gtk::ResponseType::Accept {
                    let name = name_entry.text().trim().to_string();
                    let repo_url = repo_entry.text().trim().to_string();
                    let install_path = path_entry.text().trim().to_string();
                    let version_ref = version_entry.text().trim().to_string();
                    let notes = notes_entry.text().trim().to_string();

                    if !name.is_empty() && !repo_url.is_empty() {
                        let mut store = store_for_add.borrow_mut();
                        store.profiles.push(FileProfile {
                            name: name.clone(),
                            repo_url,
                            install_path,
                            version_ref,
                            notes,
                        });
                        store.selected = Some(name);
                        save_file_profile_store(&store);
                    }

                    if let Some(refresh) = refresh_holder_for_add.borrow().as_ref() {
                        refresh();
                    }
                }

                dialog.close();
                stack_for_add.set_visible_child_name("files");
            });
        });

        files_button_row.append(&open_files_button);
        files_button_row.append(&add_file_button);

        let entry = Entry::new();
        entry.set_placeholder_text(Some("https://github.com/username/dotfiles"));

        let button_row = Box::new(Orientation::Horizontal, 10);
        let open_button = Button::with_label("Open GitHub Link");
        let copy_button = Button::with_label("Copy git clone Command");

        let parent = self.window.clone();
        let entry_for_open = entry.clone();
        open_button.connect_clicked(move |_| {
            let url = entry_for_open.text().to_string();
            open_uri(&parent, &url);
        });

        let parent = self.window.clone();
        let entry_for_copy = entry.clone();
        copy_button.connect_clicked(move |_| {
            let url = entry_for_copy.text().trim().to_string();

            if url.is_empty() {
                show_message_dialog(
                    &parent,
                    gtk::MessageType::Warning,
                    "Missing Link",
                    "Please paste a GitHub link first.",
                );
                return;
            }

            let command = format!("git clone {} ~/dotfiles", url);
            copy_text_to_clipboard(&command);
            show_message_dialog(
                &parent,
                gtk::MessageType::Info,
                "Copied",
                "The clone command has been copied to the clipboard.",
            );
        });

        button_row.append(&open_button);
        button_row.append(&copy_button);

        let hint_label = Label::new(Some(
            "Tip: if the repository includes an install script, follow the project README after cloning.",
        ));
        hint_label.set_wrap(true);
        hint_label.set_halign(gtk::Align::Start);
        hint_label.set_opacity(0.75);

        container.append(&title_label);
        container.append(&description_label);
        container.append(&files_button_row);
        container.append(&entry);
        container.append(&button_row);
        container.append(&hint_label);

        scrolled_window.set_child(Some(&container));
        self.stack
            .add_titled(&scrolled_window, Some("dotfiles"), "Dotfiles");
    }

    fn add_hyprland_install_page(&mut self) {
        let scrolled_window = ScrolledWindow::new();
        scrolled_window.set_vexpand(true);
        scrolled_window.set_hexpand(true);

        let container = Box::new(Orientation::Vertical, 14);
        container.set_margin_top(16);
        container.set_margin_bottom(16);
        container.set_margin_start(16);
        container.set_margin_end(16);

        let title_label = Label::new(Some("Hyprland Updates"));
        title_label.set_markup("<b>Hyprland Updates</b>");
        title_label.set_halign(gtk::Align::Start);

        let description_label = Label::new(Some(
            "Use the buttons below to install or update Hyprland. The GUI detects your Linux distribution and runs the matching package-manager action automatically. Leave the version fields empty for the latest release, or enter a branch, tag, commit SHA, or NixOS flake ref to pin a specific version.",
        ));
        description_label.set_wrap(true);
        description_label.set_halign(gtk::Align::Start);
        description_label.set_opacity(0.8);

        let hyprland_version_label = Label::new(Some("Hyprland version / ref"));
        hyprland_version_label.set_halign(gtk::Align::Start);
        hyprland_version_label.set_opacity(0.85);

        let hyprland_version_entry = Entry::new();
        hyprland_version_entry.set_placeholder_text(Some(
            "Optional: nixpkgs/release-20.09 or github:NixOS/nixpkgs/<ref> (NixOS only)",
        ));

        let software_version_label = Label::new(Some("GUI version / ref"));
        software_version_label.set_halign(gtk::Align::Start);
        software_version_label.set_opacity(0.85);

        let software_version_entry = Entry::new();
        software_version_entry.set_placeholder_text(Some("Optional: branch, tag, or commit SHA"));

        let version_help_label = Label::new(Some(
            "Examples: `main`, `v0.1.0`, `dc92648`, or `github:NixOS/nixpkgs/<ref>` on NixOS.",
        ));
        version_help_label.set_wrap(true);
        version_help_label.set_halign(gtk::Align::Start);
        version_help_label.set_opacity(0.72);

        let install_hyprland_button = Button::with_label("Install Hyprland");
        let update_hyprland_button = Button::with_label("Update Hyprland");
        let update_software_button = Button::with_label("Update Software");

        let parent = self.window.clone();
        let hyprland_version_for_install = hyprland_version_entry.clone();
        install_hyprland_button.connect_clicked(move |_| {
            let version_ref = entry_text_or_none(&hyprland_version_for_install);
            install_hyprland_from_gui(&parent, version_ref.as_deref());
        });

        let parent = self.window.clone();
        let hyprland_version_for_update = hyprland_version_entry.clone();
        update_hyprland_button.connect_clicked(move |_| {
            let version_ref = entry_text_or_none(&hyprland_version_for_update);
            update_hyprland_from_gui(&parent, version_ref.as_deref());
        });

        let parent = self.window.clone();
        let software_version_for_update = software_version_entry.clone();
        update_software_button.connect_clicked(move |_| {
            let version_ref = entry_text_or_none(&software_version_for_update);
            update_software_from_github(&parent, version_ref.as_deref());
        });

        let button_row = Box::new(Orientation::Horizontal, 10);
        button_row.append(&install_hyprland_button);
        button_row.append(&update_hyprland_button);
        button_row.append(&update_software_button);

        let checklist_label = Label::new(Some(
            "Recommended path: choose a version or ref if needed, then click the install, Hyprland update, or software update button.",
        ));
        checklist_label.set_wrap(true);
        checklist_label.set_halign(gtk::Align::Start);
        checklist_label.set_opacity(0.75);

        container.append(&title_label);
        container.append(&description_label);
        container.append(&hyprland_version_label);
        container.append(&hyprland_version_entry);
        container.append(&software_version_label);
        container.append(&software_version_entry);
        container.append(&version_help_label);
        container.append(&button_row);
        container.append(&checklist_label);

        scrolled_window.set_child(Some(&container));
        self.stack
            .add_titled(&scrolled_window, Some("hyprland-install"), "Hyprland Install");
    }

    pub fn load_landing_pages(&mut self, note: &str) {
        self.config_widgets.clear();
        self.changed_options.borrow_mut().clear();

        self.rebuild_navigation();
        self.add_setup_overview_page(note);
        self.add_dotfiles_page();
        self.add_files_page();
        self.add_hyprland_install_page();
    }

    pub fn setup_config_buttons(gui: Rc<RefCell<ConfigGUI>>) {
        let gui_clone = Rc::clone(&gui);
        gui.borrow().load_config_button.connect_clicked(move |_| {
            let gui = Rc::clone(&gui_clone);
            glib::MainContext::default().spawn_local(async move {
                let file_chooser = gtk::FileChooserDialog::new(
                    Some("Load HyprGUI Config"),
                    Some(&gui.borrow().window),
                    gtk::FileChooserAction::Open,
                    &[
                        ("Cancel", gtk::ResponseType::Cancel),
                        ("Open", gtk::ResponseType::Accept),
                    ],
                );

                if file_chooser.run_future().await == gtk::ResponseType::Accept {
                    if let Some(file) = file_chooser.file() {
                        if let Some(path) = file.path() {
                            gui.borrow_mut().load_hyprgui_config(&path);
                        }
                    }
                }
                file_chooser.close();
            });
        });

        let gui_clone = Rc::clone(&gui);
        gui.borrow().save_config_button.connect_clicked(move |_| {
            let gui = Rc::clone(&gui_clone);
            glib::MainContext::default().spawn_local(async move {
                let file_chooser = gtk::FileChooserDialog::new(
                    Some("Save HyprGUI Config"),
                    Some(&gui.borrow().window),
                    gtk::FileChooserAction::Save,
                    &[
                        ("Cancel", gtk::ResponseType::Cancel),
                        ("Save", gtk::ResponseType::Accept),
                    ],
                );

                file_chooser.set_current_name("hyprgui_config.json");

                if file_chooser.run_future().await == gtk::ResponseType::Accept {
                    if let Some(file) = file_chooser.file() {
                        if let Some(path) = file.path() {
                            gui.borrow_mut().save_hyprgui_config(&path);
                        }
                    }
                }
                file_chooser.close();
            });
        });
    }

    fn load_hyprgui_config(&mut self, path: &PathBuf) {
        match fs::read_to_string(path) {
            Ok(content) => {
                if let Ok(config) = serde_json::from_str::<HashMap<String, String>>(&content) {
                    for (key, value) in config {
                        let parts: Vec<&str> = key.split(':').collect();
                        if parts.len() >= 2 {
                            let category = parts[0].to_string();
                            let name = parts[1..].join(":");
                            if let Some(widget) = self.config_widgets.get(&category) {
                                if let Some(option_widget) = widget.options.get(&name) {
                                    self.set_widget_value(option_widget, &value);
                                    self.changed_options
                                        .borrow_mut()
                                        .insert((category, name), value);
                                }
                            }
                        }
                    }
                    self.custom_info_popup(
                        "Config Loaded",
                        "HyprGUI configuration loaded successfully.",
                        false,
                    );
                } else {
                    self.custom_error_popup(
                        "Invalid Config",
                        "Failed to parse the configuration file.",
                        false,
                    );
                }
            }
            Err(e) => {
                self.custom_error_popup(
                    "Loading Failed",
                    &format!("Failed to read the configuration file: {}", e),
                    false,
                );
            }
        }
    }

    fn save_hyprgui_config(&mut self, path: &PathBuf) {
        let config: HashMap<String, String> = self
            .changed_options
            .borrow()
            .iter()
            .map(|((category, name), value)| (format!("{}:{}", category, name), value.clone()))
            .collect();

        match serde_json::to_string_pretty(&config) {
            Ok(json) => match fs::write(path, json) {
                Ok(_) => {
                    self.custom_info_popup(
                        "Config Saved",
                        "HyprGUI configuration saved successfully.",
                        false,
                    );
                }
                Err(e) => {
                    self.custom_error_popup(
                        "Saving Failed",
                        &format!("Failed to write the configuration file: {}", e),
                        false,
                    );
                }
            },
            Err(e) => {
                self.custom_error_popup(
                    "Serialization Failed",
                    &format!("Failed to serialize the configuration: {}", e),
                    false,
                );
            }
        }
    }

    fn set_widget_value(&self, widget: &Widget, value: &str) {
        if let Some(spin_button) = widget.downcast_ref::<SpinButton>() {
            if let Ok(float_value) = value.parse::<f64>() {
                spin_button.set_value(float_value);
            }
        } else if let Some(entry) = widget.downcast_ref::<Entry>() {
            entry.set_text(value);
        } else if let Some(switch) = widget.downcast_ref::<Switch>() {
            switch.set_active(value == "true");
        } else if let Some(color_button) = widget.downcast_ref::<ColorButton>() {
            let dummy_config = HyprlandConfig::new();
            if let Some((red, green, blue, alpha)) = dummy_config.parse_color(value) {
                color_button.set_rgba(&gdk::RGBA::new(
                    red as f32,
                    green as f32,
                    blue as f32,
                    alpha as f32,
                ));
            }
        } else if let Some(dropdown) = widget.downcast_ref::<DropDown>() {
            let model = dropdown.model().unwrap();
            for i in 0..model.n_items() {
                if let Some(item) = model.item(i) {
                    if let Some(string_object) = item.downcast_ref::<gtk::StringObject>() {
                        if string_object.string() == value {
                            dropdown.set_selected(i);
                            break;
                        }
                    }
                }
            }
        }
    }

    pub fn custom_info_popup(&mut self, title: &str, text: &str, modal: bool) {
        let dialog = MessageDialog::builder()
            .message_type(gtk::MessageType::Info)
            .buttons(gtk::ButtonsType::Ok)
            .title(title)
            .text(text)
            .modal(modal)
            .build();

        dialog.connect_response(|dialog, _| {
            dialog.close();
        });

        dialog.show();
    }

    pub fn custom_error_popup(&mut self, title: &str, text: &str, modal: bool) {
        let dialog = MessageDialog::builder()
            .message_type(gtk::MessageType::Error)
            .buttons(gtk::ButtonsType::Ok)
            .title(title)
            .text(text)
            .modal(modal)
            .build();

        dialog.connect_response(|dialog, _| {
            dialog.close();
        });

        dialog.show();
    }

    pub fn custom_error_popup_critical(&mut self, title: &str, text: &str, modal: bool) {
        let dialog = MessageDialog::builder()
            .message_type(gtk::MessageType::Error)
            .buttons(gtk::ButtonsType::Ok)
            .title(title)
            .text(text)
            .modal(modal)
            .build();

        dialog.connect_response(|_, _| {
            std::process::exit(1);
        });

        dialog.show();
    }

    pub fn load_config(&mut self, config: &HyprlandConfig) {
        self.config_widgets.clear();
        self.content_box.set_visible(true);

        self.rebuild_navigation();
        self.add_setup_overview_page("Your Hyprland config file is ready to edit.");
        self.add_dotfiles_page();
        self.add_files_page();
        self.add_hyprland_install_page();

        let categories = [
            ("General", "general"),
            ("Decoration", "decoration"),
            ("Animations", "animations"),
            ("Input", "input"),
            ("Gestures", "gestures"),
            ("Misc", "misc"),
            ("Binds", "binds"),
            ("Group", "group"),
            ("Layouts", "layouts"),
            ("XWayland", "xwayland"),
            ("OpenGL", "opengl"),
            ("Render", "render"),
            ("Cursor", "cursor"),
            ("Debug", "debug"),
        ];

        for (display_name, category) in &categories {
            let widget = ConfigWidget::new(category);
            self.stack
                .add_titled(&widget.scrolled_window, Some(category), display_name);
            self.config_widgets.insert(category.to_string(), widget);
        }

        for (_, category) in &categories {
            if let Some(widget) = self.config_widgets.get(*category) {
                widget.load_config(config, category, self.changed_options.clone());
            }
        }

        self.changed_options.borrow_mut().clear();
    }

    pub fn get_changes(&self) -> Rc<RefCell<HashMap<(String, String), String>>> {
        self.changed_options.clone()
    }

    pub fn apply_changes(&self, config: &mut HyprlandConfig) {
        let changes = self.changed_options.borrow();
        for (category, widget) in &self.config_widgets {
            for (name, widget) in &widget.options {
                if let Some(value) = changes.get(&(category.to_string(), name.to_string())) {
                    let formatted_value =
                        if let Some(color_button) = widget.downcast_ref::<ColorButton>() {
                            let rgba = color_button.rgba();
                            format!(
                                "rgba({:02X}{:02X}{:02X}{:02X})",
                                (rgba.red() * 255.0) as u8,
                                (rgba.green() * 255.0) as u8,
                                (rgba.blue() * 255.0) as u8,
                                (rgba.alpha() * 255.0) as u8
                            )
                        } else {
                            value.clone()
                        };

                    if !formatted_value.is_empty() {
                        if category == "layouts" {
                            let parts: Vec<&str> = name.split(':').collect();
                            if parts.len() == 2 {
                                config.add_entry(
                                    parts[0],
                                    &format!("{} = {}", parts[1], formatted_value),
                                );
                            }
                        } else if name.contains(':') {
                            let parts: Vec<&str> = name.split(':').collect();
                            if parts.len() == 2 {
                                config.add_entry(
                                    &format!("{}.{}", category, parts[0]),
                                    &format!("{} = {}", parts[1], formatted_value),
                                );
                            }
                        } else {
                            config.add_entry(category, &format!("{} = {}", name, formatted_value));
                        }
                    }
                }
            }
        }
    }
}

fn get_option_limits(name: &str, description: &str) -> (f64, f64, f64) {
    match name {
        "border_size" => (0.0, 10.0, 1.0),
        "gaps_in" | "gaps_out" | "gaps_workspaces" => (0.0, 50.0, 1.0),
        "resize_corner" => (0.0, 4.0, 1.0),
        "rounding" => (0.0, 20.0, 1.0),
        "active_opacity" | "inactive_opacity" | "fullscreen_opacity" => (0.0, 1.0, 0.1),
        "shadow_range" => (0.0, 50.0, 1.0),
        "shadow_render_power" => (1.0, 4.0, 1.0),
        "shadow_scale" => (0.0, 1.0, 0.1),
        "dim_strength" | "dim_special" | "dim_around" => (0.0, 1.0, 0.1),
        "blur:size" => (1.0, 20.0, 1.0),
        "blur:passes" => (1.0, 10.0, 1.0),
        "blur:noise" => (0.0, 1.0, 0.01),
        "blur:contrast" => (0.0, 2.0, 0.1),
        "blur:brightness" => (0.0, 2.0, 0.1),
        "blur:vibrancy" | "blur:vibrancy_darkness" => (0.0, 1.0, 0.1),
        "blur:popups_ignorealpha" => (0.0, 1.0, 0.1),
        "sensitivity" => (-1.0, 1.0, 0.1),
        "scroll_button" => (0.0, 9.0, 1.0),
        "scroll_factor" => (0.1, 10.0, 0.1),
        "follow_mouse" => (0.0, 3.0, 1.0),
        "float_switch_override_focus" => (0.0, 2.0, 1.0),
        "workspace_swipe_fingers" => (2.0, 5.0, 1.0),
        "workspace_swipe_distance" => (100.0, 500.0, 10.0),
        "workspace_swipe_min_speed_to_force" => (0.0, 100.0, 1.0),
        "workspace_swipe_cancel_ratio" => (0.0, 1.0, 0.1),
        "workspace_swipe_direction_lock_threshold" => (0.0, 50.0, 1.0),
        "drag_into_group" => (0.0, 2.0, 1.0),
        "force_default_wallpaper" => (-1.0, 2.0, 1.0),
        "vrr" => (0.0, 2.0, 1.0),
        "render_ahead_safezone" => (0.0, 10.0, 1.0),
        "new_window_takes_over_fullscreen" => (0.0, 2.0, 1.0),
        "initial_workspace_tracking" => (0.0, 2.0, 1.0),
        "render_unfocused_fps" => (1.0, 60.0, 1.0),
        "scroll_event_delay" => (0.0, 1000.0, 10.0),
        "workspace_center_on" => (0.0, 1.0, 1.0),
        "focus_preferred_method" => (0.0, 1.0, 1.0),
        "force_introspection" => (0.0, 2.0, 1.0),
        "explicit_sync" | "explicit_sync_kms" => (0.0, 2.0, 1.0),
        "min_refresh_rate" => (1.0, 240.0, 1.0),
        "hotspot_padding" => (0.0, 10.0, 1.0),
        "inactive_timeout" => (0.0, 60.0, 1.0),
        "zoom_factor" => (1.0, 5.0, 0.1),
        "damage_tracking" => (0.0, 2.0, 1.0),
        "watchdog_timeout" => (0.0, 60.0, 1.0),
        "error_limit" => (1.0, 100.0, 1.0),
        "error_position" => (0.0, 1.0, 1.0),
        "repeat_rate" => (1.0, 100.0, 1.0),
        "repeat_delay" => (100.0, 2000.0, 100.0),
        "touchpad:scroll_factor" => (0.1, 10.0, 0.1),
        "tablet:transform" => (0.0, 7.0, 1.0),
        "off_window_axis_events" => (0.0, 3.0, 1.0),
        "emulate_discrete_scroll" => (0.0, 2.0, 1.0),
        "focus_on_close" => (0.0, 1.0, 1.0),
        "groupbar:font_size" => (6.0, 32.0, 1.0),
        "groupbar:height" => (10.0, 50.0, 1.0),
        "groupbar:priority" => (0.0, 10.0, 1.0),
        "manual_crash" => (0.0, 1.0, 1.0),
        _ => {
            if description.contains("[0.0 - 1.0]") {
                (0.0, 1.0, 0.1)
            } else if description.contains("[0/1]") {
                (0.0, 1.0, 1.0)
            } else if description.contains("[0/1/2]") {
                (0.0, 2.0, 1.0)
            } else if name.contains("opacity") || name.contains("ratio") {
                (0.0, 1.0, 0.1)
            } else {
                (0.0, 50.0, 1.0)
            }
        }
    }
}

pub struct ConfigWidget {
    options: HashMap<String, Widget>,
    scrolled_window: ScrolledWindow,
}

impl ConfigWidget {
    fn new(category: &str) -> Self {
        let scrolled_window = ScrolledWindow::new();
        scrolled_window.set_vexpand(false);
        scrolled_window.set_propagate_natural_height(true);

        let container = Box::new(Orientation::Vertical, 0);
        container.set_margin_start(20);
        container.set_margin_end(20);
        container.set_margin_top(20);
        container.set_margin_bottom(20);

        scrolled_window.set_child(Some(&container));

        let mut options = HashMap::new();

        let first_section = Rc::new(RefCell::new(true));

        match category {
            "general" => {
                Self::add_section(
                    &container,
                    "General Settings",
                    "Configure general behavior.",
                    first_section.clone(),
                );

                Self::add_section(
                    &container,
                    "Layout",
                    "Choose the default layout.",
                    first_section.clone(),
                );
                add_dropdown_option(
                    &container,
                    &mut options,
                    "layout",
                    "Layout",
                    "which layout to use.",
                    &["dwindle", "master"],
                );
                Self::add_section(
                    &container,
                    "Gaps",
                    "Change gaps in & out, workspaces.",
                    first_section.clone(),
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "gaps_in",
                    "Gaps In",
                    "gaps between windows, also supports css style gaps (top, right, bottom, left -> 5,10,15,20)",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "gaps_out",
                    "Gaps Out",
                    "gaps between windows and monitor edges, also supports css style gaps (top, right, bottom, left -> 5,10,15,20)",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "gaps_workspaces",
                    "Gaps Workspaces",
                    "gaps between workspaces. Stacks with gaps_out.",
                );

                Self::add_section(
                    &container,
                    "Borders",
                    "Size, resize, floating...",
                    first_section.clone(),
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "border_size",
                    "Border Size",
                    "size of the border around windows",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "no_border_on_floating",
                    "No Border on Floating",
                    "disable borders for floating windows",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "resize_on_border",
                    "Resize on Border",
                    "enables resizing windows by clicking and dragging on borders and gaps",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "extend_border_grab_area",
                    "Extend Border Grab Area",
                    "extends the area around the border where you can click and drag on, only used when general:resize_on_border is on.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "hover_icon_on_border",
                    "Hover Icon on Border",
                    "show a cursor icon when hovering over borders, only used when general:resize_on_border is on.",
                );

                Self::add_section(
                    &container,
                    "Colors",
                    "Change borders colors.",
                    first_section.clone(),
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "col.inactive_border",
                    "Inactive Border Color",
                    "border color for inactive windows",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "col.active_border",
                    "Active Border Color",
                    "border color for the active window",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "col.nogroup_border",
                    "No Group Border Color",
                    "inactive border color for window that cannot be added to a group (see denywindowfromgroup dispatcher)",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "col.nogroup_border_active",
                    "No Group Active Border Color",
                    "active border color for window that cannot be added to a group",
                );
            }
            "decoration" => {
                Self::add_section(
                    &container,
                    "Window Decoration",
                    "Configure window appearance.",
                    first_section.clone(),
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "rounding",
                    "Rounding",
                    "rounded corners' radius (in layout px)",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "active_opacity",
                    "Active Opacity",
                    "opacity of active windows. [0.0 - 1.0]",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "inactive_opacity",
                    "Inactive Opacity",
                    "opacity of inactive windows. [0.0 - 1.0]",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "fullscreen_opacity",
                    "Fullscreen Opacity",
                    "opacity of fullscreen windows. [0.0 - 1.0]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "drop_shadow",
                    "Drop Shadow",
                    "enable drop shadows on windows",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "shadow_range",
                    "Shadow Range",
                    "Shadow range (\"size\") in layout px",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "shadow_render_power",
                    "Shadow Render Power",
                    "in what power to render the falloff (more power, the faster the falloff) [1 - 4]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "shadow_ignore_window",
                    "Shadow Ignore Window",
                    "if true, the shadow will not be rendered behind the window itself, only around it.",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "col.shadow",
                    "Shadow Color",
                    "shadow's color. Alpha dictates shadow's opacity.",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "col.shadow_inactive",
                    "Inactive Shadow Color",
                    "inactive shadow color. (if not set, will fall back to col.shadow)",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "shadow_offset",
                    "Shadow Offset",
                    "shadow's rendering offset. Format: \"x y\" (e.g. \"0 0\")",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "shadow_scale",
                    "Shadow Scale",
                    "shadow's scale. [0.0 - 1.0]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "dim_inactive",
                    "Dim Inactive",
                    "enables dimming of inactive windows",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "dim_strength",
                    "Dim Strength",
                    "how much inactive windows should be dimmed [0.0 - 1.0]",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "dim_special",
                    "Dim Special",
                    "how much to dim the rest of the screen by when a special workspace is open. [0.0 - 1.0]",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "dim_around",
                    "Dim Around",
                    "how much the dimaround window rule should dim by. [0.0 - 1.0]",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "screen_shader",
                    "Screen Shader",
                    "a path to a custom shader to be applied at the end of rendering. See examples/screenShader.frag for an example.",
                );

                Self::add_section(
                    &container,
                    "Blur",
                    "Configure blur settings.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "blur:enabled",
                    "Blur Enabled",
                    "enable kawase window background blur",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "blur:size",
                    "Blur Size",
                    "blur size (distance)",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "blur:passes",
                    "Blur Passes",
                    "the amount of passes to perform",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "blur:ignore_opacity",
                    "Blur Ignore Opacity",
                    "make the blur layer ignore the opacity of the window",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "blur:new_optimizations",
                    "Blur New Optimizations",
                    "whether to enable further optimizations to the blur. Recommended to leave on, as it will massively improve performance.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "blur:xray",
                    "Blur X-Ray",
                    "if enabled, floating windows will ignore tiled windows in their blur. Only available if blur_new_optimizations is true. Will reduce overhead on floating blur significantly.",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "blur:noise",
                    "Blur Noise",
                    "how much noise to apply. [0.0 - 1.0]",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "blur:contrast",
                    "Blur Contrast",
                    "contrast modulation for blur. [0.0 - 2.0]",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "blur:brightness",
                    "Blur Brightness",
                    "brightness modulation for blur. [0.0 - 2.0]",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "blur:vibrancy",
                    "Blur Vibrancy",
                    "Increase saturation of blurred colors. [0.0 - 1.0]",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "blur:vibrancy_darkness",
                    "Blur Vibrancy Darkness",
                    "How strong the effect of vibrancy is on dark areas . [0.0 - 1.0]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "blur:special",
                    "Blur Special",
                    "whether to blur behind the special workspace (note: expensive)",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "blur:popups",
                    "Blur Popups",
                    "whether to blur popups (e.g. right-click menus)",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "blur:popups_ignorealpha",
                    "Blur Popups Ignore Alpha",
                    "works like ignorealpha in layer rules. If pixel opacity is below set value, will not blur. [0.0 - 1.0]",
                );
            }
            "animations" => {
                Self::add_section(
                    &container,
                    "Animation Settings",
                    "Configure animation behavior.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "enabled",
                    "Enable Animations",
                    "Enables animations.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "first_launch_animation",
                    "First Launch Animation",
                    "Enables the first launch animation.",
                );
            }
            "input" => {
                Self::add_section(
                    &container,
                    "Input Settings",
                    "Configure input devices.",
                    first_section.clone(),
                );
                Self::add_section(
                    &container,
                    "Keyboard Settings",
                    "Configure keyboard behavior.",
                    first_section.clone(),
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "kb_model",
                    "Keyboard Model",
                    "Appropriate XKB keymap parameter.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "kb_layout",
                    "Keyboard Layout",
                    "Appropriate XKB keymap parameter",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "kb_variant",
                    "Keyboard Variant",
                    "Appropriate XKB keymap parameter",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "kb_options",
                    "Keyboard Options",
                    "Appropriate XKB keymap parameter",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "kb_rules",
                    "Keyboard Rules",
                    "Appropriate XKB keymap parameter",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "kb_file",
                    "Keyboard File",
                    "If you prefer, you can use a path to your custom .xkb file.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "numlock_by_default",
                    "Numlock by Default",
                    "Engage numlock by default.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "resolve_binds_by_sym",
                    "Resolve Binds by Symbol",
                    "Determines how keybinds act when multiple layouts are used. If false, keybinds will always act as if the first specified layout is active. If true, keybinds specified by symbols are activated when you type the respective symbol with the current layout.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "repeat_rate",
                    "Repeat Rate",
                    "The repeat rate for held-down keys, in repeats per second.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "repeat_delay",
                    "Repeat Delay",
                    "Delay before a held-down key is repeated, in milliseconds.",
                );

                Self::add_section(
                    &container,
                    "Mouse Settings",
                    "Configure mouse behavior.",
                    first_section.clone(),
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "sensitivity",
                    "Sensitivity",
                    "Sets the mouse input sensitivity. Value is clamped to the range -1.0 to 1.0.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "accel_profile",
                    "Acceleration Profile",
                    "Sets the cursor acceleration profile. Can be one of adaptive, flat. Can also be custom, see below. Leave empty to use libinput's default mode for your input device.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "force_no_accel",
                    "Force No Acceleration",
                    "Force no cursor acceleration. This bypasses most of your pointer settings to get as raw of a signal as possible. Enabling this is not recommended due to potential cursor desynchronization.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "left_handed",
                    "Left Handed",
                    "Switches RMB and LMB",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "scroll_method",
                    "Scroll Method",
                    "Sets the scroll method. Can be one of 2fg (2 fingers), edge, on_button_down, no_scroll.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "scroll_button",
                    "Scroll Button",
                    "Sets the scroll button. Has to be an int, cannot be a string. Check wev if you have any doubts regarding the ID. 0 means default.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "scroll_button_lock",
                    "Scroll Button Lock",
                    "If the scroll button lock is enabled, the button does not need to be held down. Pressing and releasing the button toggles the button lock, which logically holds the button down or releases it. While the button is logically held down, motion events are converted to scroll events.",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "scroll_factor",
                    "Scroll Factor",
                    "Multiplier added to scroll movement for external mice. Note that there is a separate setting for touchpad scroll_factor.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "natural_scroll",
                    "Natural Scroll",
                    "Inverts scrolling direction. When enabled, scrolling moves content directly, rather than manipulating a scrollbar.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "follow_mouse",
                    "Follow Mouse",
                    "Specify if and how cursor movement should affect window focus. 0 - Cursor movement will not change focus, 1 - Cursor movement will always change focus to the window under the cursor, 2 - Cursor focus will be detached from keyboard focus, 3 - Cursor focus will be completely separate from keyboard focus. [0/1/2/3]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "mouse_refocus",
                    "Mouse Refocus",
                    "If disabled, mouse focus won't switch to the hovered window unless the mouse crosses a window boundary when follow_mouse=1.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "scroll_points",
                    "Scroll Points",
                    "Sets the scroll acceleration profile, when accel_profile is set to custom. Has to be in the form <step> <points>. Leave empty to have a flat scroll curve.",
                );

                Self::add_section(
                    &container,
                    "Focus Settings",
                    "Configure focus behavior.",
                    first_section.clone(),
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "focus_on_close",
                    "Focus on Close",
                    "Controls the window focus behavior when a window is closed. 0 - focus will shift to the next window candidate, 1 - focus will shift to the window under the cursor. [0/1]",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "float_switch_override_focus",
                    "Float Switch Override Focus",
                    "If enabled, focus will change to the window under the cursor when changing from tiled-to-floating and vice versa. 0 - disabled, 1 - enabled, 2 - focus will also follow mouse on float-to-float switches. [0/1/2]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "special_fallthrough",
                    "Special Fallthrough",
                    "if enabled, having only floating windows in the special workspace will not block focusing windows in the regular workspace.",
                );

                Self::add_section(
                    &container,
                    "Touchpad Settings",
                    "Configure touchpad behavior.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "touchpad:disable_while_typing",
                    "Disable While Typing",
                    "Disables the touchpad while typing.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "touchpad:natural_scroll",
                    "Natural Scroll",
                    "Enables natural scroll.",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "touchpad:scroll_factor",
                    "Scroll Factor",
                    "The scroll factor.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "touchpad:middle_button_emulation",
                    "Middle Button Emulation",
                    "Emulates the middle button.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "touchpad:tap_button_map",
                    "Tap Button Map",
                    "The tap button map.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "touchpad:clickfinger_behavior",
                    "Clickfinger Behavior",
                    "The clickfinger behavior.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "touchpad:tap-to-click",
                    "Tap to Click",
                    "Enables tap to click.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "touchpad:drag_lock",
                    "Drag Lock",
                    "Enables drag lock.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "touchpad:tap-and-drag",
                    "Tap and Drag",
                    "Enables tap and drag.",
                );

                Self::add_section(
                    &container,
                    "Touchscreen Settings",
                    "Configure touchscreen behavior.",
                    first_section.clone(),
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "touchdevice:transform",
                    "Transform",
                    "The transform.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "touchdevice:output",
                    "Output",
                    "The output.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "touchdevice:enabled",
                    "Enabled",
                    "Enables the touchdevice.",
                );

                Self::add_section(
                    &container,
                    "Tablet Settings",
                    "Configure tablet behavior.",
                    first_section.clone(),
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "tablet:transform",
                    "Transform",
                    "The transform.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "tablet:output",
                    "Output",
                    "The output.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "tablet:region_position",
                    "Region Position",
                    "The region position.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "tablet:region_size",
                    "Region Size",
                    "The region size.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "tablet:relative_input",
                    "Relative Input",
                    "Enables relative input.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "tablet:left_handed",
                    "Left Handed",
                    "Enables left handed mode.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "tablet:active_area_size",
                    "Active Area Size",
                    "The active area size.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "tablet:active_area_position",
                    "Active Area Position",
                    "The active area position.",
                );

                Self::add_section(
                    &container,
                    "Miscellaneous Input Settings",
                    "Other input-related settings.",
                    first_section.clone(),
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "off_window_axis_events",
                    "Off Window Axis Events",
                    "Handles axis events around a focused window. 0 - ignores axis events, 1 - sends out-of-bound coordinates, 2 - fakes pointer coordinates to the closest point inside the window, 3 - warps the cursor to the closest point inside the window [0/1/2/3]",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "emulate_discrete_scroll",
                    "Emulate Discrete Scroll",
                    "Emulates discrete scrolling from high resolution scrolling events. 0 - disables it, 1 - enables handling of non-standard events only, 2 - force enables all scroll wheel events to be handled [0/1/2]",
                );
            }
            "gestures" => {
                Self::add_section(
                    &container,
                    "Gesture Settings",
                    "Configure gesture behavior.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "workspace_swipe",
                    "Workspace Swipe",
                    "enable workspace swipe gesture on touchpad",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "workspace_swipe_fingers",
                    "Workspace Swipe Fingers",
                    "how many fingers for the touchpad gesture",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "workspace_swipe_min_fingers",
                    "Workspace Swipe Min Fingers",
                    "if enabled, workspace_swipe_fingers is considered the minimum number of fingers to swipe",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "workspace_swipe_distance",
                    "Workspace Swipe Distance",
                    "in px, the distance of the touchpad gesture",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "workspace_swipe_touch",
                    "Workspace Swipe Touch",
                    "enable workspace swiping from the edge of a touchscreen",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "workspace_swipe_invert",
                    "Workspace Swipe Invert",
                    "invert the direction (touchpad only)",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "workspace_swipe_touch_invert",
                    "Workspace Swipe Touch Invert",
                    "invert the direction (touchscreen only)",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "workspace_swipe_min_speed_to_force",
                    "Workspace Swipe Min Speed to Force",
                    "minimum speed in px per timepoint to force the change ignoring cancel_ratio. Setting to 0 will disable this mechanic.",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "workspace_swipe_cancel_ratio",
                    "Workspace Swipe Cancel Ratio",
                    "how much the swipe has to proceed in order to commence it. (0.7 -> if > 0.7 * distance, switch, if less, revert) [0.0 - 1.0]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "workspace_swipe_create_new",
                    "Workspace Swipe Create New",
                    "whether a swipe right on the last workspace should create a new one.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "workspace_swipe_direction_lock",
                    "Workspace Swipe Direction Lock",
                    "if enabled, switching direction will be locked when you swipe past the direction_lock_threshold (touchpad only).",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "workspace_swipe_direction_lock_threshold",
                    "Workspace Swipe Direction Lock Threshold",
                    "in px, the distance to swipe before direction lock activates (touchpad only).",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "workspace_swipe_forever",
                    "Workspace Swipe Forever",
                    "if enabled, swiping will not clamp at the neighboring workspaces but continue to the further ones.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "workspace_swipe_use_r",
                    "Workspace Swipe Use R",
                    "if enabled, swiping will use the r prefix instead of the m prefix for finding workspaces.",
                );
            }

            "group" => {
                Self::add_section(
                    &container,
                    "Group Settings",
                    "Configure group behavior.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "auto_group",
                    "Auto Group",
                    "whether new windows will be automatically grouped into the focused unlocked group",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "insert_after_current",
                    "Insert After Current",
                    "whether new windows in a group spawn after current or at group tail",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "focus_removed_window",
                    "Focus Removed Window",
                    "whether Hyprland should focus on the window that has just been moved out of the group",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "drag_into_group",
                    "Drag Into Group",
                    "whether dragging a window into a unlocked group will merge them. 0 - disabled, 1 - enabled, 2 - only when dragging into the groupbar [0/1/2]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "merge_groups_on_drag",
                    "Merge Groups on Drag",
                    "whether window groups can be dragged into other groups",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "merge_floated_into_tiled_on_groupbar",
                    "Merge Floated Into Tiled on Groupbar",
                    "whether dragging a floating window into a tiled window groupbar will merge them",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "col.border_active",
                    "Active Border Color",
                    "active group border color",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "col.border_inactive",
                    "Inactive Border Color",
                    "inactive (out of focus) group border color",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "col.border_locked_active",
                    "Locked Active Border Color",
                    "active locked group border color",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "col.border_locked_inactive",
                    "Locked Inactive Border Color",
                    "inactive locked group border color",
                );
                Self::add_section(
                    &container,
                    "Groupbar Settings",
                    "Configure groupbar behavior.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "groupbar:enabled",
                    "Enabled",
                    "enables groupbars",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "groupbar:font_family",
                    "Font Family",
                    "font used to display groupbar titles, use misc:font_family if not specified",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "groupbar:font_size",
                    "Font Size",
                    "font size of groupbar title",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "groupbar:gradients",
                    "Gradients",
                    "enables gradients",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "groupbar:height",
                    "Height",
                    "height of the groupbar",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "groupbar:stacked",
                    "Stacked",
                    "render the groupbar as a vertical stack",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "groupbar:priority",
                    "Priority",
                    "sets the decoration priority for groupbars",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "groupbar:render_titles",
                    "Render Titles",
                    "whether to render titles in the group bar decoration",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "groupbar:scrolling",
                    "Scrolling",
                    "whether scrolling in the groupbar changes group active window",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "groupbar:text_color",
                    "Text Color",
                    "controls the group bar text color",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "groupbar:col.active",
                    "Active Color",
                    "active group border color",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "groupbar:col.inactive",
                    "Inactive Color",
                    "inactive (out of focus) group border color",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "groupbar:col.locked_active",
                    "Locked Active Color",
                    "active locked group border color",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "groupbar:col.locked_inactive",
                    "Locked Inactive Color",
                    "inactive locked group border color",
                );
            }
            "misc" => {
                Self::add_section(
                    &container,
                    "Miscellaneous Settings",
                    "Configure miscellaneous behavior.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "disable_hyprland_logo",
                    "Disable Hyprland Logo",
                    "disables the random Hyprland logo / anime girl background. :(",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "disable_splash_rendering",
                    "Disable Splash Rendering",
                    "disables the Hyprland splash rendering. (requires a monitor reload to take effect)",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "col.splash",
                    "Splash Color",
                    "Changes the color of the splash text (requires a monitor reload to take effect).",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "font_family",
                    "Font Family",
                    "Set the global default font to render the text including debug fps/notification, config error messages and etc., selected from system fonts.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "splash_font_family",
                    "Splash Font Family",
                    "Changes the font used to render the splash text, selected from system fonts (requires a monitor reload to take effect).",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "force_default_wallpaper",
                    "Force Default Wallpaper",
                    "Enforce any of the 3 default wallpapers. -1 - random, 0 or 1 - disables the anime background, 2 - enables anime background. [-1/0/1/2]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "vfr",
                    "VFR",
                    "controls the VFR status of Hyprland. Heavily recommended to leave enabled to conserve resources.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "vrr",
                    "VRR",
                    "Controls the VRR (Adaptive Sync) of your monitors. 0 - off, 1 - on, 2 - fullscreen only [0/1/2]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "mouse_move_enables_dpms",
                    "Mouse Move Enables DPMS",
                    "If DPMS is set to off, wake up the monitors if the mouse moves.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "key_press_enables_dpms",
                    "Key Press Enables DPMS",
                    "If DPMS is set to off, wake up the monitors if a key is pressed.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "always_follow_on_dnd",
                    "Always Follow on DnD",
                    "Will make mouse focus follow the mouse when drag and dropping. Recommended to leave it enabled, especially for people using focus follows mouse at 0.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "layers_hog_keyboard_focus",
                    "Layers Hog Keyboard Focus",
                    "If true, will make keyboard-interactive layers keep their focus on mouse move (e.g. wofi, bemenu)",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "animate_manual_resizes",
                    "Animate Manual Resizes",
                    "If true, will animate manual window resizes/moves",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "animate_mouse_windowdragging",
                    "Animate Mouse Window Dragging",
                    "If true, will animate windows being dragged by mouse, note that this can cause weird behavior on some curves",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "disable_autoreload",
                    "Disable Autoreload",
                    "If true, the config will not reload automatically on save, and instead needs to be reloaded with hyprctl reload. Might save on battery.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "enable_swallow",
                    "Enable Swallow",
                    "Enable window swallowing",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "swallow_regex",
                    "Swallow Regex",
                    "The class regex to be used for windows that should be swallowed (usually, a terminal). To know more about the list of regex which can be used use this cheatsheet.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "swallow_exception_regex",
                    "Swallow Exception Regex",
                    "The title regex to be used for windows that should not be swallowed by the windows specified in swallow_regex (e.g. wev). The regex is matched against the parent (e.g. Kitty) window's title on the assumption that it changes to whatever process it's running.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "focus_on_activate",
                    "Focus on Activate",
                    "Whether Hyprland should focus an app that requests to be focused (an activate request)",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "mouse_move_focuses_monitor",
                    "Mouse Move Focuses Monitor",
                    "Whether mouse moving into a different monitor should focus it",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "render_ahead_of_time",
                    "Render Ahead of Time",
                    "[Warning: buggy] starts rendering before your monitor displays a frame in order to lower latency"
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "render_ahead_safezone",
                    "Render Ahead Safezone",
                    "how many ms of safezone to add to rendering ahead of time. Recommended 1-2.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "allow_session_lock_restore",
                    "Allow Session Lock Restore",
                    "if true, will allow you to restart a lockscreen app in case it crashes (red screen of death)",
                );
                Self::add_color_option(
                    &container,
                    &mut options,
                    "background_color",
                    "Background Color",
                    "change the background color. (requires enabled disable_hyprland_logo)",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "close_special_on_empty",
                    "Close Special on Empty",
                    "close the special workspace if the last window is removed",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "new_window_takes_over_fullscreen",
                    "New Window Takes Over Fullscreen",
                    "If there is a fullscreen or maximized window, decide whether a new tiled window opened should replace it, stay behind or disable the fullscreen/maximized state. 0 - behind, 1 - takes over, 2 - unfullscreen/unmaxize [0/1/2]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "exit_window_retains_fullscreen",
                    "Exit Window Retains Fullscreen",
                    "if true, closing a fullscreen window makes the next focused window fullscreen",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "initial_workspace_tracking",
                    "Initial Workspace Tracking",
                    "If enabled, windows will open on the workspace they were invoked on. 0 - disabled, 1 - single-shot, 2 - persistent (all children too) [0/1/2]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "middle_click_paste",
                    "Middle Click Paste",
                    "whether to enable middle-click-paste (aka primary selection)",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "render_unfocused_fps",
                    "Render Unfocused FPS",
                    "the maximum limit for renderunfocused windows' fps in the background (see also Window-Rules - renderunfocused)",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "disable_xdg_env_checks",
                    "Disable XDG Environment Checks",
                    "disable the warning if XDG environment is externally managed",
                );
            }
            "binds" => {
                Self::add_section(
                    &container,
                    "Bind Settings",
                    "Configure keybinding behavior.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "pass_mouse_when_bound",
                    "Pass Mouse When Bound",
                    "If disabled, will not pass the mouse events to apps / dragging windows around if a keybind has been triggered.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "scroll_event_delay",
                    "Scroll Event Delay",
                    "In ms, how many ms to wait after a scroll event to allow passing another one for the binds.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "workspace_back_and_forth",
                    "Workspace Back and Forth",
                    "If enabled, an attempt to switch to the currently focused workspace will instead switch to the previous workspace.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "allow_workspace_cycles",
                    "Allow Workspace Cycles",
                    "If enabled, workspaces don't forget their previous workspace, so cycles can be created.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "workspace_center_on",
                    "Workspace Center On",
                    "Whether switching workspaces should center the cursor on the workspace (0) or on the last active window for that workspace (1). [0/1]",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "focus_preferred_method",
                    "Focus Preferred Method",
                    "Sets the preferred focus finding method when using focuswindow/movewindow/etc with a direction. 0 - history (recent have priority), 1 - length (longer shared edges have priority) [0/1]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "ignore_group_lock",
                    "Ignore Group Lock",
                    "If enabled, dispatchers like moveintogroup, moveoutofgroup and movewindoworgroup will ignore lock per group.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "movefocus_cycles_fullscreen",
                    "Movefocus Cycles Fullscreen",
                    "If enabled, when on a fullscreen window, movefocus will cycle fullscreen, if not, it will move the focus in a direction.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "disable_keybind_grabbing",
                    "Disable Keybind Grabbing",
                    "If enabled, apps that request keybinds to be disabled (e.g. VMs) will not be able to do so.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "window_direction_monitor_fallback",
                    "Window Direction Monitor Fallback",
                    "If enabled, moving a window or focus over the edge of a monitor with a direction will move it to the next monitor in that direction.",
                );
            }
            "xwayland" => {
                Self::add_section(
                    &container,
                    "XWayland Settings",
                    "Configure XWayland behavior.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "enabled",
                    "Enabled",
                    "Allow running applications using X11.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "use_nearest_neighbor",
                    "Use Nearest Neighbor",
                    "Uses the nearest neighbor filtering for xwayland apps, making them pixelated rather than blurry.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "force_zero_scaling",
                    "Force Zero Scaling",
                    "Forces a scale of 1 on xwayland windows on scaled displays.",
                );
            }
            "opengl" => {
                Self::add_section(
                    &container,
                    "OpenGL Settings",
                    "Configure OpenGL behavior.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "nvidia_anti_flicker",
                    "Nvidia Anti Flicker",
                    "Reduces flickering on nvidia at the cost of possible frame drops on lower-end GPUs.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "force_introspection",
                    "Force Introspection",
                    "Forces introspection at all times. Introspection is aimed at reducing GPU usage in certain cases, but might cause graphical glitches on nvidia. 0 - nothing, 1 - force always on, 2 - force always on if nvidia [0/1/2]",
                );
            }
            "render" => {
                Self::add_section(
                    &container,
                    "Render Settings",
                    "Configure render behavior.",
                    first_section.clone(),
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "explicit_sync",
                    "Explicit Sync",
                    "Whether to enable explicit sync support. 0 - no, 1 - yes, 2 - auto based on the gpu driver [0/1/2]",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "explicit_sync_kms",
                    "Explicit Sync KMS",
                    "Whether to enable explicit sync support for the KMS layer. Requires explicit_sync to be enabled. 0 - no, 1 - yes, 2 - auto based on the gpu driver [0/1/2]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "direct_scanout",
                    "Direct Scanout",
                    "Enables direct scanout. Direct scanout attempts to reduce lag when there is only one fullscreen application on a screen.",
                );
            }
            "cursor" => {
                Self::add_section(
                    &container,
                    "Cursor Settings",
                    "Configure cursor behavior.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "sync_gsettings_theme",
                    "Sync GSettings Theme",
                    "Sync xcursor theme with gsettings.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "no_hardware_cursors",
                    "No Hardware Cursors",
                    "Disables hardware cursors.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "no_break_fs_vrr",
                    "No Break FS VRR",
                    "Disables scheduling new frames on cursor movement for fullscreen apps with VRR enabled to avoid framerate spikes.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "min_refresh_rate",
                    "Min Refresh Rate",
                    "Minimum refresh rate for cursor movement when no_break_fs_vrr is active.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "hotspot_padding",
                    "Hotspot Padding",
                    "The padding, in logical px, between screen edges and the cursor.",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "inactive_timeout",
                    "Inactive Timeout",
                    "In seconds, after how many seconds of cursor's inactivity to hide it.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "no_warps",
                    "No Warps",
                    "If true, will not warp the cursor in many cases.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "persistent_warps",
                    "Persistent Warps",
                    "When a window is refocused, the cursor returns to its last position relative to that window.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "warp_on_change_workspace",
                    "Warp on Change Workspace",
                    "If true, move the cursor to the last focused window after changing the workspace.",
                );
                Self::add_string_option(
                    &container,
                    &mut options,
                    "default_monitor",
                    "Default Monitor",
                    "The name of a default monitor for the cursor to be set to on startup.",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "zoom_factor",
                    "Zoom Factor",
                    "The factor to zoom by around the cursor. Like a magnifying glass.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "zoom_rigid",
                    "Zoom Rigid",
                    "Whether the zoom should follow the cursor rigidly or loosely.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "enable_hyprcursor",
                    "Enable Hyprcursor",
                    "Whether to enable hyprcursor support.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "hide_on_key_press",
                    "Hide on Key Press",
                    "Hides the cursor when you press any key until the mouse is moved.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "hide_on_touch",
                    "Hide on Touch",
                    "Hides the cursor when the last input was a touch input until a mouse input is done.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "allow_dumb_copy",
                    "Allow Dumb Copy",
                    "Makes HW cursors work on Nvidia, at the cost of a possible hitch whenever the image changes.",
                );
            }
            "debug" => {
                Self::add_section(
                    &container,
                    "Debug Settings",
                    "Configure debug behavior.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "overlay",
                    "Overlay",
                    "Print the debug performance overlay.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "damage_blink",
                    "Damage Blink",
                    "(epilepsy warning!) Flash areas updated with damage tracking.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "disable_logs",
                    "Disable Logs",
                    "Disable logging to a file.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "disable_time",
                    "Disable Time",
                    "Disables time logging.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "damage_tracking",
                    "Damage Tracking",
                    "Redraw only the needed bits of the display. Do not change. 0 - none, 1 - monitor, 2 - full (default) [0/1/2]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "enable_stdout_logs",
                    "Enable Stdout Logs",
                    "Enables logging to stdout.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "manual_crash",
                    "Manual Crash",
                    "Set to 1 and then back to 0 to crash Hyprland.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "suppress_errors",
                    "Suppress Errors",
                    "If true, do not display config file parsing errors.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "watchdog_timeout",
                    "Watchdog Timeout",
                    "Sets the timeout in seconds for watchdog to abort processing of a signal of the main thread. Set to 0 to disable.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "disable_scale_checks",
                    "Disable Scale Checks",
                    "Disables verification of the scale factors. Will result in pixel alignment and rounding errors.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "error_limit",
                    "Error Limit",
                    "Limits the number of displayed config file parsing errors.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "error_position",
                    "Error Position",
                    "Sets the position of the error bar. 0 - top, 1 - bottom [0/1]",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "colored_stdout_logs",
                    "Colored Stdout Logs",
                    "Enables colors in the stdout logs.",
                );
            }
            "layouts" => {
                Self::add_section(
                    &container,
                    "Layout Settings",
                    "Configure layout behavior.",
                    first_section.clone(),
                );

                Self::add_section(
                    &container,
                    "Dwindle Layout",
                    "Configure Dwindle layout settings.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "dwindle:pseudotile",
                    "Pseudotile",
                    "Enable pseudotiling. Pseudotiled windows retain their floating size when tiled.",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "dwindle:force_split",
                    "Force Split",
                    "0 -> split follows mouse, 1 -> always split to the left (new = left or top) 2 -> always split to the right (new = right or bottom)",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "dwindle:preserve_split",
                    "Preserve Split",
                    "If enabled, the split (side/top) will not change regardless of what happens to the container.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "dwindle:smart_split",
                    "Smart Split",
                    "If enabled, allows a more precise control over the window split direction based on the cursor's position.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "dwindle:smart_resizing",
                    "Smart Resizing",
                    "If enabled, resizing direction will be determined by the mouse's position on the window.",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "dwindle:permanent_direction_override",
                    "Permanent Direction Override",
                    "If enabled, makes the preselect direction persist until changed or disabled.",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "dwindle:special_scale_factor",
                    "Special Scale Factor",
                    "Specifies the scale factor of windows on the special workspace [0 - 1]",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "dwindle:split_width_multiplier",
                    "Split Width Multiplier",
                    "Specifies the auto-split width multiplier",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "dwindle:use_active_for_splits",
                    "Use Active for Splits",
                    "Whether to prefer the active window or the mouse position for splits",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "dwindle:default_split_ratio",
                    "Default Split Ratio",
                    "The default split ratio on window open. 1 means even 50/50 split. [0.1 - 1.9]",
                );
                Self::add_int_option(
                    &container,
                    &mut options,
                    "dwindle:split_bias",
                    "Split Bias",
                    "Specifies which window will receive the larger half of a split. [0/1/2]",
                );

                Self::add_section(
                    &container,
                    "Master Layout",
                    "Configure Master layout settings.",
                    first_section.clone(),
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "master:allow_small_split",
                    "Allow Small Split",
                    "Enable adding additional master windows in a horizontal split style",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "master:special_scale_factor",
                    "Special Scale Factor",
                    "The scale of the special workspace windows. [0.0 - 1.0]",
                );
                Self::add_float_option(
                    &container,
                    &mut options,
                    "master:mfact",
                    "Master Factor",
                    "The size as a percentage of the master window. [0.0 - 1.0]",
                );
                add_dropdown_option(
                    &container,
                    &mut options,
                    "master:new_status",
                    "New Window Status",
                    "Determines how new windows are added to the layout.",
                    &["master", "slave", "inherit"],
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "master:new_on_top",
                    "New on Top",
                    "Whether a newly open window should be on the top of the stack",
                );
                add_dropdown_option(
                    &container,
                    &mut options,
                    "master:new_on_active",
                    "New on Active",
                    "Place new window relative to the focused window",
                    &["before", "after", "none"],
                );
                add_dropdown_option(
                    &container,
                    &mut options,
                    "master:orientation",
                    "Orientation",
                    "Default placement of the master area",
                    &["left", "right", "top", "bottom", "center"],
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "master:inherit_fullscreen",
                    "Inherit Fullscreen",
                    "Inherit fullscreen status when cycling/swapping to another window",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "master:always_center_master",
                    "Always Center Master",
                    "Keep the master window centered when using center orientation",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "master:smart_resizing",
                    "Smart Resizing",
                    "If enabled, resizing direction will be determined by the mouse's position on the window",
                );
                Self::add_bool_option(
                    &container,
                    &mut options,
                    "master:drop_at_cursor",
                    "Drop at Cursor",
                    "When enabled, dragging and dropping windows will put them at the cursor position",
                );
            }
            _ => {
                Self::add_section(
                    &container,
                    &format!("{} Settings", category),
                    &format!("Configure {} behavior.", category),
                    first_section.clone(),
                );
            }
        }

        ConfigWidget {
            options,
            scrolled_window,
        }
    }

    fn add_section(
        container: &Box,
        title: &str,
        description: &str,
        first_section: Rc<RefCell<bool>>,
    ) {
        let section_box = Box::new(Orientation::Vertical, 5);
        section_box.set_margin_top(15);
        section_box.set_margin_bottom(10);

        let title_label = Label::new(Some(title));
        let desc_label = Label::new(Some(description));

        if *first_section.borrow() {
            title_label.set_halign(gtk::Align::Center);
            desc_label.set_halign(gtk::Align::Center);
            title_label.set_hexpand(true);
            desc_label.set_hexpand(true);
            *first_section.borrow_mut() = false;
        } else {
            title_label.set_halign(gtk::Align::Start);
            desc_label.set_halign(gtk::Align::Start);
        }

        title_label.set_markup(&format!("<b>{}</b>", title));
        section_box.append(&title_label);

        desc_label.set_opacity(0.7);
        section_box.append(&desc_label);

        let frame = Frame::new(None);
        frame.set_margin_top(10);
        section_box.append(&frame);

        container.append(&section_box);
    }

    fn add_int_option(
        container: &Box,
        options: &mut HashMap<String, Widget>,
        name: &str,
        label: &str,
        description: &str,
    ) {
        let hbox = Box::new(Orientation::Horizontal, 10);
        hbox.set_margin_start(10);
        hbox.set_margin_end(10);
        hbox.set_margin_top(5);
        hbox.set_margin_bottom(5);

        let label_box = Box::new(Orientation::Horizontal, 5);
        label_box.set_hexpand(true);

        let label_widget = Label::new(Some(label));
        label_widget.set_halign(gtk::Align::Start);

        let tooltip_button = Button::new();
        let question_mark_icon = Image::from_icon_name("dialog-question-symbolic");
        tooltip_button.set_child(Some(&question_mark_icon));
        tooltip_button.set_has_frame(false);

        let popover = Popover::new();
        let description_label = Label::new(Some(description));
        description_label.set_margin_top(5);
        description_label.set_margin_bottom(5);
        description_label.set_margin_start(5);
        description_label.set_margin_end(5);
        popover.set_child(Some(&description_label));
        popover.set_position(gtk::PositionType::Right);

        tooltip_button.connect_clicked(move |button| {
            popover.set_parent(button);
            popover.popup();
        });

        label_box.append(&label_widget);
        label_box.append(&tooltip_button);

        let (min, max, step) = get_option_limits(name, description);
        let spin_button = SpinButton::with_range(min, max, step);
        spin_button.set_digits(0);
        spin_button.set_halign(gtk::Align::End);
        spin_button.set_width_request(100);

        hbox.append(&label_box);
        hbox.append(&spin_button);

        container.append(&hbox);

        options.insert(name.to_string(), spin_button.upcast());
    }

    fn add_bool_option(
        container: &Box,
        options: &mut HashMap<String, Widget>,
        name: &str,
        label: &str,
        description: &str,
    ) {
        let hbox = Box::new(Orientation::Horizontal, 10);
        hbox.set_margin_start(10);
        hbox.set_margin_end(10);
        hbox.set_margin_top(5);
        hbox.set_margin_bottom(5);

        let label_box = Box::new(Orientation::Horizontal, 5);
        label_box.set_hexpand(true);

        let label_widget = Label::new(Some(label));
        label_widget.set_halign(gtk::Align::Start);

        let tooltip_button = Button::new();
        let question_mark_icon = Image::from_icon_name("dialog-question-symbolic");
        tooltip_button.set_child(Some(&question_mark_icon));
        tooltip_button.set_has_frame(false);

        let popover = Popover::new();
        let description_label = Label::new(Some(description));
        description_label.set_margin_top(5);
        description_label.set_margin_bottom(5);
        description_label.set_margin_start(5);
        description_label.set_margin_end(5);
        popover.set_child(Some(&description_label));
        popover.set_position(gtk::PositionType::Right);

        tooltip_button.connect_clicked(move |button| {
            popover.set_parent(button);
            popover.popup();
        });

        label_box.append(&label_widget);
        label_box.append(&tooltip_button);

        let switch = Switch::new();
        switch.set_halign(gtk::Align::End);
        switch.set_valign(gtk::Align::Center);

        hbox.append(&label_box);
        hbox.append(&switch);

        container.append(&hbox);

        options.insert(name.to_string(), switch.upcast());
    }

    fn add_float_option(
        container: &Box,
        options: &mut HashMap<String, Widget>,
        name: &str,
        label: &str,
        description: &str,
    ) {
        let hbox = Box::new(Orientation::Horizontal, 10);
        hbox.set_margin_start(10);
        hbox.set_margin_end(10);
        hbox.set_margin_top(5);
        hbox.set_margin_bottom(5);

        let label_box = Box::new(Orientation::Horizontal, 5);
        label_box.set_hexpand(true);

        let label_widget = Label::new(Some(label));
        label_widget.set_halign(gtk::Align::Start);

        let tooltip_button = Button::new();
        let question_mark_icon = Image::from_icon_name("dialog-question-symbolic");
        tooltip_button.set_child(Some(&question_mark_icon));
        tooltip_button.set_has_frame(false);

        let popover = Popover::new();
        let description_label = Label::new(Some(description));
        description_label.set_margin_top(5);
        description_label.set_margin_bottom(5);
        description_label.set_margin_start(5);
        description_label.set_margin_end(5);
        popover.set_child(Some(&description_label));
        popover.set_position(gtk::PositionType::Right);

        tooltip_button.connect_clicked(move |button| {
            popover.set_parent(button);
            popover.popup();
        });

        label_box.append(&label_widget);
        label_box.append(&tooltip_button);

        let (min, max, step) = get_option_limits(name, description);
        let spin_button = SpinButton::with_range(min, max, step);
        spin_button.set_digits(2);
        spin_button.set_halign(gtk::Align::End);
        spin_button.set_width_request(100);

        hbox.append(&label_box);
        hbox.append(&spin_button);

        container.append(&hbox);

        options.insert(name.to_string(), spin_button.upcast());
    }

    fn add_string_option(
        container: &Box,
        options: &mut HashMap<String, Widget>,
        name: &str,
        label: &str,
        description: &str,
    ) {
        let hbox = Box::new(Orientation::Horizontal, 10);
        hbox.set_margin_start(10);
        hbox.set_margin_end(10);
        hbox.set_margin_top(5);
        hbox.set_margin_bottom(5);

        let label_box = Box::new(Orientation::Horizontal, 5);
        label_box.set_hexpand(true);

        let label_widget = Label::new(Some(label));
        label_widget.set_halign(gtk::Align::Start);

        let tooltip_button = Button::new();
        let question_mark_icon = Image::from_icon_name("dialog-question-symbolic");
        tooltip_button.set_child(Some(&question_mark_icon));
        tooltip_button.set_has_frame(false);

        let popover = Popover::new();
        let description_label = Label::new(Some(description));
        description_label.set_margin_top(5);
        description_label.set_margin_bottom(5);
        description_label.set_margin_start(5);
        description_label.set_margin_end(5);
        popover.set_child(Some(&description_label));
        popover.set_position(gtk::PositionType::Right);

        tooltip_button.connect_clicked(move |button| {
            popover.set_parent(button);
            popover.popup();
        });

        label_box.append(&label_widget);
        label_box.append(&tooltip_button);

        let entry = Entry::new();
        entry.set_halign(gtk::Align::End);
        entry.set_width_request(100);

        hbox.append(&label_box);
        hbox.append(&entry);

        container.append(&hbox);

        options.insert(name.to_string(), entry.upcast());
    }

    fn add_color_option(
        container: &Box,
        options: &mut HashMap<String, Widget>,
        name: &str,
        label: &str,
        description: &str,
    ) {
        let hbox = Box::new(Orientation::Horizontal, 10);
        hbox.set_margin_start(10);
        hbox.set_margin_end(10);
        hbox.set_margin_top(5);
        hbox.set_margin_bottom(5);

        let label_box = Box::new(Orientation::Horizontal, 5);
        label_box.set_hexpand(true);

        let label_widget = Label::new(Some(label));
        label_widget.set_halign(gtk::Align::Start);

        let tooltip_button = Button::new();
        let question_mark_icon = Image::from_icon_name("dialog-question-symbolic");
        tooltip_button.set_child(Some(&question_mark_icon));
        tooltip_button.set_has_frame(false);

        let popover = Popover::new();
        let description_label = Label::new(Some(description));
        description_label.set_margin_top(5);
        description_label.set_margin_bottom(5);
        description_label.set_margin_start(5);
        description_label.set_margin_end(5);
        popover.set_child(Some(&description_label));
        popover.set_position(gtk::PositionType::Right);

        tooltip_button.connect_clicked(move |button| {
            popover.set_parent(button);
            popover.popup();
        });

        label_box.append(&label_widget);
        label_box.append(&tooltip_button);

        let color_button = ColorButton::new();
        color_button.set_halign(gtk::Align::End);

        hbox.append(&label_box);
        hbox.append(&color_button);

        container.append(&hbox);

        options.insert(name.to_string(), color_button.upcast());
    }

    fn load_config(
        &self,
        config: &HyprlandConfig,
        category: &str,
        changed_options: Rc<RefCell<HashMap<(String, String), String>>>,
    ) {
        for (name, widget) in &self.options {
            let value = self.extract_value(config, category, name);
            if let Some(spin_button) = widget.downcast_ref::<gtk::SpinButton>() {
                let float_value = value.parse::<f64>().unwrap_or(0.0);
                spin_button.set_value(float_value);
                let category = category.to_string();
                let name = name.to_string();
                let changed_options = changed_options.clone();
                spin_button.connect_value_changed(move |sb| {
                    let mut changes = changed_options.borrow_mut();
                    let new_value = sb.value().to_string();
                    changes.insert((category.clone(), name.clone()), new_value);
                });
            } else if let Some(entry) = widget.downcast_ref::<Entry>() {
                entry.set_text(&value);
                let category = category.to_string();
                let name = name.to_string();
                let changed_options = changed_options.clone();
                entry.connect_changed(move |entry| {
                    let mut changes = changed_options.borrow_mut();
                    let new_value = entry.text().to_string();
                    changes.insert((category.clone(), name.clone()), new_value);
                });
            } else if let Some(switch) = widget.downcast_ref::<Switch>() {
                switch.set_active(value == "true");
                let category = category.to_string();
                let name = name.to_string();
                let changed_options = changed_options.clone();
                switch.connect_active_notify(move |sw| {
                    let mut changes = changed_options.borrow_mut();
                    let new_value = sw.is_active().to_string();
                    changes.insert((category.clone(), name.clone()), new_value);
                });
            } else if let Some(color_button) = widget.downcast_ref::<ColorButton>() {
                if let Some((red, green, blue, alpha)) = config.parse_color(&value) {
                    color_button.set_rgba(&gdk::RGBA::new(
                        red as f32,
                        green as f32,
                        blue as f32,
                        alpha as f32,
                    ));
                }
                let category = category.to_string();
                let name = name.to_string();
                let changed_options = changed_options.clone();
                color_button.connect_color_set(move |cb| {
                    let mut changes = changed_options.borrow_mut();
                    let new_color = cb.rgba();
                    let new_value = format!(
                        "rgba({:02X}{:02X}{:02X}{:02X})",
                        (new_color.red() * 255.0) as u8,
                        (new_color.green() * 255.0) as u8,
                        (new_color.blue() * 255.0) as u8,
                        (new_color.alpha() * 255.0) as u8
                    );
                    changes.insert((category.clone(), name.clone()), new_value);
                });
            } else if let Some(dropdown) = widget.downcast_ref::<gtk::DropDown>() {
                let model = dropdown.model().unwrap();
                for i in 0..model.n_items() {
                    if let Some(item) = model.item(i) {
                        if let Some(string_object) = item.downcast_ref::<gtk::StringObject>() {
                            if string_object.string() == value {
                                dropdown.set_selected(i);
                                break;
                            }
                        }
                    }
                }
                let category = category.to_string();
                let name = name.to_string();
                let changed_options = changed_options.clone();
                dropdown.connect_selected_notify(move |dd| {
                    let mut changes = changed_options.borrow_mut();
                    if let Some(selected) = dd.selected_item() {
                        if let Some(string_object) = selected.downcast_ref::<gtk::StringObject>() {
                            let new_value = string_object.string().to_string();
                            changes.insert((category.clone(), name.clone()), new_value);
                        }
                    }
                });
            }
        }
    }

    fn extract_value(&self, config: &HyprlandConfig, _category: &str, name: &str) -> String {
        let config_str = config.to_string();
        for line in config_str.lines() {
            if line.trim().starts_with(&format!("{} = ", name)) {
                return line
                    .split('=')
                    .nth(1)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
            }
        }
        String::new()
    }
}
