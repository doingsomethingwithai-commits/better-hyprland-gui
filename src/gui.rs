use gtk::{
    gdk, gio, glib, prelude::*, Application, ApplicationWindow, Box, Button, ColorButton,
    CssProvider, DropDown, Entry, Frame, Grid, HeaderBar, Image, Label, MessageDialog, Orientation,
    Paned, Popover, ScrolledWindow, Separator, SpinButton, Stack, StackSidebar, StringList, Switch,
    Widget,
};

use hyprparser::HyprlandConfig;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};

const SOFTWARE_REPO_URL: &str =
    "https://github.com/doingsomethingwithai-commits/better-hyprland-gui.git";

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
    active_profile: Option<String>,
    active_entries: Vec<String>,
}

fn icon_image(icon_name: &str) -> Image {
    let image = Image::from_icon_name(icon_name);
    image.set_valign(gtk::Align::Center);
    image.set_vexpand(false);
    image
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
    let question_mark_icon = icon_image("dialog-question-symbolic");
    tooltip_button.set_child(Some(&question_mark_icon));
    tooltip_button.set_has_frame(false);

    let popover = Popover::new();
    let description_label = Label::new(Some(description));
    description_label.set_margin_top(5);
    description_label.set_margin_bottom(5);
    description_label.set_margin_start(5);
    description_label.set_margin_end(5);
    description_label.set_wrap(true);
    description_label.set_max_width_chars(56);
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
    let dialog = gtk::Dialog::builder()
        .transient_for(parent)
        .title(title)
        .modal(true)
        .default_width(760)
        .default_height(520)
        .build();

    let content = dialog.content_area();
    content.set_spacing(10);
    content.set_margin_top(14);
    content.set_margin_bottom(14);
    content.set_margin_start(14);
    content.set_margin_end(14);

    let heading = Box::new(Orientation::Horizontal, 8);
    let icon_name = match message_type {
        gtk::MessageType::Error => "dialog-error-symbolic",
        gtk::MessageType::Warning => "dialog-warning-symbolic",
        gtk::MessageType::Question => "dialog-question-symbolic",
        _ => "dialog-information-symbolic",
    };
    heading.append(&icon_image(icon_name));
    let heading_label = Label::new(Some(title));
    heading_label.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(title)));
    heading_label.set_halign(gtk::Align::Start);
    heading.append(&heading_label);
    content.append(&heading);

    let scroller = ScrolledWindow::new();
    scroller.set_vexpand(true);
    scroller.set_hexpand(true);
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);

    let text_view = gtk::TextView::new();
    text_view.set_editable(false);
    text_view.set_cursor_visible(false);
    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);
    text_view.set_vexpand(true);
    text_view.set_hexpand(true);
    text_view.buffer().set_text(text);
    scroller.set_child(Some(&text_view));
    content.append(&scroller);

    dialog.add_button("Copy", gtk::ResponseType::Help);
    dialog.add_button("Close", gtk::ResponseType::Close);

    let clipboard = text_view.display().clipboard();
    let copy_text = text.to_string();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Help {
            clipboard.set_text(&copy_text);
        } else {
            dialog.close();
        }
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

    if !trimmed.starts_with("https://") && !trimmed.starts_with("http://") {
        show_message_dialog(
            parent,
            gtk::MessageType::Warning,
            "Unsupported Link",
            "Only HTTP and HTTPS links can be opened from the GUI.",
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

fn git_repo_root(path: &Path) -> Option<PathBuf> {
    let output = git_command()
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        None
    } else {
        Some(PathBuf::from(root))
    }
}

fn find_repo_root(start: PathBuf) -> Option<PathBuf> {
    let mut current = Some(start.as_path());
    while let Some(path) = current {
        if let Some(root) = git_repo_root(path) {
            return Some(root);
        }
        current = path.parent();
    }
    None
}

fn is_hyprgui_repo(path: &Path) -> bool {
    let manifest = path.join("Cargo.toml");
    let main_source = path.join("src").join("main.rs");

    main_source.is_file()
        && fs::read_to_string(manifest)
            .map(|content| {
                content.lines().any(|line| {
                    let compact = line.split_whitespace().collect::<String>();
                    compact == "name=\"hyprgui\""
                })
            })
            .unwrap_or(false)
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn git_command() -> Command {
    if Path::new("/usr/bin/git").is_file() {
        Command::new("/usr/bin/git")
    } else {
        Command::new("git")
    }
}

fn default_app_dir() -> Option<PathBuf> {
    home_dir().map(|path| {
        path.join(".local")
            .join("share")
            .join("better-hyprland-gui")
    })
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
    let mut candidates = Vec::new();

    if let Some(app_dir) = env::var_os("APP_DIR") {
        candidates.push(Some(PathBuf::from(app_dir)));
    }

    if let Some(repo_dir) = env::var_os("HYPRGUI_REPO_DIR") {
        candidates.push(Some(PathBuf::from(repo_dir)));
    }

    candidates.extend(install_state_repo_dirs().into_iter().map(Some));
    candidates.push(
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf())),
    );
    candidates.push(default_app_dir());
    candidates.push(std::env::current_dir().ok());

    for candidate in candidates.into_iter().flatten() {
        if let Some(repo_root) = find_repo_root(candidate) {
            if is_hyprgui_repo(&repo_root) {
                return Some(repo_root);
            }
        }
    }

    None
}

fn executable_from_env_or_path(name: &str, env_key: &str) -> Option<PathBuf> {
    if let Some(value) = env::var_os(env_key) {
        let path = PathBuf::from(value);
        if path.exists() {
            return Some(path);
        }
    }

    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|path| path.join(name))
            .find(|candidate| candidate.exists())
    })
}

fn cargo_binary() -> Option<PathBuf> {
    executable_from_env_or_path("cargo", "CARGO")
        .or_else(|| home_dir().map(|path| path.join(".cargo").join("bin").join("cargo")))
        .filter(|path| path.exists())
}

fn git_current_branch(repo_dir: &Path) -> Result<Option<String>, String> {
    let output = git_command()
        .arg("-C")
        .arg(repo_dir)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_err(|err| format!("Failed to start git branch detection: {err}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        Ok(None)
    } else {
        Ok(Some(branch))
    }
}

fn entry_text_or_none(entry: &Entry) -> Option<String> {
    let text = entry.text().trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn validate_version_ref(version_ref: &str) -> Result<&str, String> {
    let trimmed = version_ref.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('-')
        || trimmed.chars().any(char::is_control)
        || trimmed.chars().any(char::is_whitespace)
    {
        return Err(
            "The requested version ref is empty or contains unsafe characters.".to_string(),
        );
    }

    Ok(trimmed)
}

fn ensure_repo_clean(repo_dir: &Path) -> Result<(), String> {
    ensure_git_repository(repo_dir)?;

    let output = git_command()
        .arg("-C")
        .arg(repo_dir)
        .args(["status", "--porcelain"])
        .output()
        .map_err(|err| format!("Failed to inspect repository changes: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "Git could not inspect the repository state.\n\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    if !output.stdout.is_empty() {
        return Err(
            "The repository contains local changes. Commit or stash them before updating."
                .to_string(),
        );
    }

    Ok(())
}

fn ensure_git_repository(repo_dir: &Path) -> Result<(), String> {
    if git_repo_root(repo_dir).is_some() {
        Ok(())
    } else {
        Err(format!(
            "The configured application directory is not a Git checkout.\n\nPath: {}\n\nSet APP_DIR or HYPRGUI_REPO_DIR to the cloned Better Hyprland GUI repository, then try again.",
            repo_dir.display()
        ))
    }
}

fn fetch_repo(repo_dir: &Path) -> Result<(), String> {
    ensure_git_repository(repo_dir)?;

    let output = git_command()
        .arg("-C")
        .arg(repo_dir)
        .args(["fetch", "--prune", "--tags", "origin"])
        .output()
        .map_err(|err| format!("Failed to start git fetch: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "The git fetch command failed.\n\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn sync_software_repo_from_github(
    repo_dir: &Path,
    version_ref: Option<&str>,
) -> Result<(), String> {
    ensure_git_repository(repo_dir)?;

    let remote_update = git_command()
        .arg("-C")
        .arg(repo_dir)
        .args(["remote", "set-url", "origin", SOFTWARE_REPO_URL])
        .output()
        .map_err(|error| format!("Failed to configure the GitHub remote: {error}"))?;
    if !remote_update.status.success() {
        return Err(format!(
            "Could not configure the GitHub remote.\n\n{}",
            String::from_utf8_lossy(&remote_update.stderr)
        ));
    }

    fetch_repo(repo_dir)?;

    let reset = git_command()
        .arg("-C")
        .arg(repo_dir)
        .args(["reset", "--hard", "origin/main"])
        .output()
        .map_err(|error| format!("Failed to reset the local checkout to GitHub: {error}"))?;
    if !reset.status.success() {
        return Err(format!(
            "Could not reset the local checkout to GitHub main.\n\n{}",
            String::from_utf8_lossy(&reset.stderr)
        ));
    }

    if let Some(version_ref) = version_ref {
        checkout_repo_ref(repo_dir, version_ref)?;
    }

    Ok(())
}

fn checkout_repo_ref(repo_dir: &Path, version_ref: &str) -> Result<(), String> {
    ensure_repo_clean(repo_dir)?;
    let version_ref = validate_version_ref(version_ref)?;
    let candidates = [
        version_ref.to_string(),
        format!("origin/{version_ref}"),
        format!("refs/tags/{version_ref}"),
    ];

    let mut last_error = String::new();

    for candidate in candidates {
        let output = git_command()
            .arg("-C")
            .arg(repo_dir)
            .args(["checkout", "--detach", &candidate])
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

fn run_background_task<F>(
    parent: &ApplicationWindow,
    button: Option<&Button>,
    running_label: &str,
    success_title: &str,
    success_message: &str,
    failure_title: &str,
    task: F,
) where
    F: FnOnce() -> Result<(), String> + Send + 'static,
{
    run_background_task_with_completion(
        parent,
        button,
        running_label,
        success_title,
        success_message,
        failure_title,
        None,
        task,
    );
}

fn run_background_task_with_completion<F>(
    parent: &ApplicationWindow,
    button: Option<&Button>,
    running_label: &str,
    success_title: &str,
    success_message: &str,
    failure_title: &str,
    on_success: Option<std::boxed::Box<dyn FnOnce() + 'static>>,
    task: F,
) where
    F: FnOnce() -> Result<(), String> + Send + 'static,
{
    let button_state = button.map(|button| {
        let original_label = button.label().map(|label| label.to_string());
        button.set_sensitive(false);
        button.set_label(running_label);
        (button.clone(), original_label)
    });

    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(task());
    });

    let parent = parent.clone();
    let success_title = success_title.to_string();
    let success_message = success_message.to_string();
    let failure_title = failure_title.to_string();
    let mut on_success = on_success;

    glib::timeout_add_local(Duration::from_millis(100), move || {
        match receiver.try_recv() {
            Ok(Ok(())) => {
                restore_task_button(&button_state);
                if let Some(on_success) = on_success.take() {
                    on_success();
                } else {
                    show_install_result(&parent, &success_title, true, &success_message);
                }
                glib::ControlFlow::Break
            }
            Ok(Err(error)) => {
                restore_task_button(&button_state);
                show_install_result(&parent, &failure_title, false, &error);
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                restore_task_button(&button_state);
                show_install_result(
                    &parent,
                    &failure_title,
                    false,
                    "The background task stopped unexpectedly.",
                );
                glib::ControlFlow::Break
            }
        }
    });
}

fn restore_task_button(button_state: &Option<(Button, Option<String>)>) {
    if let Some((button, original_label)) = button_state {
        if let Some(label) = original_label {
            button.set_label(label);
        }
        button.set_sensitive(true);
    }
}

fn command_result(mut command: Command) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|err| format!("Failed to start the command: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let details = if stderr.trim().is_empty() {
            stdout
        } else if stdout.trim().is_empty() {
            stderr.clone()
        } else {
            format!("{stderr}\n{stdout}")
        };
        let lowercase_stderr = details.to_lowercase();
        if lowercase_stderr.contains("db.lck")
            || lowercase_stderr.contains("unable to lock database")
            || lowercase_stderr.contains("could not lock database")
            || (lowercase_stderr.contains("datenbank") && lowercase_stderr.contains("sperr"))
        {
            return Err(
                "Another system package manager is currently using the package database. Close the other installer or update process, wait for it to finish, and try again. Do not delete the lock file manually."
                    .to_string(),
            );
        }

        Err(format!("The command failed.\n\n{details}"))
    }
}

fn rebuild_software_from_repo(repo_dir: &Path) -> Result<(), String> {
    let cargo_path = cargo_binary().ok_or_else(|| {
        "Could not find the cargo executable. Restart the session after installing Rust, or make sure ~/.cargo/bin is available to the app."
            .to_string()
    })?;

    let mut cargo_command = Command::new(cargo_path);
    cargo_command
        .current_dir(repo_dir)
        .args(["build", "--release"]);
    command_result(cargo_command).map_err(|error| format!("The rebuild failed.\n\n{error}"))
}

fn restart_updated_application(parent: &ApplicationWindow, repo_dir: &Path) -> Result<(), String> {
    let binary_path = repo_dir.join("target").join("release").join("hyprgui");
    if !binary_path.is_file() {
        return Err(format!(
            "The updated application binary was not found at {}.",
            binary_path.display()
        ));
    }

    let old_pid = std::process::id().to_string();
    let binary_path = binary_path.to_string_lossy().into_owned();
    if Path::new("/usr/bin/systemd-run").is_file() {
        let unit_name = format!("hyprgui-restart-{old_pid}");
        let restart_script =
            "while kill -0 \"$1\" 2>/dev/null; do sleep 0.1; done; exec \"$2\"";
        Command::new("/usr/bin/systemd-run")
            .args([
                "--user",
                "--quiet",
                "--collect",
                &format!("--unit={unit_name}"),
                &format!("--setenv=APP_DIR={}", repo_dir.display()),
                &format!("--setenv=HYPRGUI_REPO_DIR={}", repo_dir.display()),
                "/bin/sh",
                "-c",
                restart_script,
                "hyprgui-restart",
                &old_pid,
                &binary_path,
            ])
            .spawn()
            .map_err(|error| format!("The updated application could not be restarted: {error}"))?;
    } else {
        Command::new(&binary_path)
            .env("APP_DIR", repo_dir)
            .env("HYPRGUI_REPO_DIR", repo_dir)
            .spawn()
            .map_err(|error| format!("The updated application could not be restarted: {error}"))?;
    }

    // The application ID stays unchanged, so the desktop shell keeps the
    // existing taskbar entry while the new process takes over.
    parent.close();
    if let Some(application) = parent.application() {
        application.quit();
    }
    Ok(())
}

fn hard_update_software_from_github(
    parent: &ApplicationWindow,
    button: &Button,
    version_ref: Option<&str>,
) {
    let Some(repo_dir) = software_repo_dir() else {
        show_message_dialog(
            parent,
            gtk::MessageType::Warning,
            "Repository Not Found",
            "I could not find a verified local checkout of Better Hyprland GUI. Set APP_DIR/HYPRGUI_REPO_DIR to the correct checkout.",
        );
        return;
    };

    let pinned_ref = version_ref
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(version_ref) = pinned_ref.as_deref() {
        if let Err(error) = validate_version_ref(version_ref) {
            show_message_dialog(
                parent,
                gtk::MessageType::Warning,
                "Invalid Version Ref",
                &error,
            );
            return;
        }
    }

    let script_path = repo_dir.join("scripts").join("hard-update.sh");
    if !script_path.is_file() {
        show_message_dialog(
            parent,
            gtk::MessageType::Error,
            "Hard Update Unavailable",
            &format!("The hard-update script was not found at {}.", script_path.display()),
        );
        return;
    }

    let app_dir = repo_dir.to_string_lossy().into_owned();
    let mut command = Command::new("bash");
    command
        .arg(&script_path)
        .env("APP_DIR", &app_dir)
        .env("NO_LAUNCH", "1");
    if let Some(version_ref) = pinned_ref {
        command.env("APP_REF", version_ref);
    }

    let restart_repo_dir = repo_dir.clone();
    let restart_parent = parent.clone();
    let restart_action: std::boxed::Box<dyn FnOnce() + 'static> = std::boxed::Box::new(move || {
        if let Err(error) = restart_updated_application(&restart_parent, &restart_repo_dir) {
            show_message_dialog(
                &restart_parent,
                gtk::MessageType::Error,
                "Restart Failed",
                &error,
            );
        }
    });

    run_background_task_with_completion(
        parent,
        Some(button),
        "Updating software…",
        "Hard Update Complete",
        "The new version was built. Restarting the application now…",
        "Hard Update Failed",
        Some(restart_action),
        move || command_result(command),
    );
}

fn hard_delete_software_from_github(parent: &ApplicationWindow, button: &Button) {
    let Some(repo_dir) = software_repo_dir() else {
        show_message_dialog(
            parent,
            gtk::MessageType::Warning,
            "Repository Not Found",
            "I could not find a verified local checkout of Better Hyprland GUI.",
        );
        return;
    };

    let script_path = repo_dir.join("scripts").join("hard-delete.sh");
    if !script_path.is_file() {
        show_message_dialog(
            parent,
            gtk::MessageType::Error,
            "Hard Delete Unavailable",
            &format!("The hard-delete script was not found at {}.", script_path.display()),
        );
        return;
    }

    let app_dir = repo_dir.to_string_lossy().into_owned();
    let mut command = Command::new("bash");
    command.arg(&script_path).env("APP_DIR", &app_dir);
    run_background_task(
        parent,
        Some(button),
        "Deleting checkout...",
        "Hard Delete Complete",
        "The application checkout was deleted. Close this window and reinstall when needed.",
        "Hard Delete Failed",
        move || command_result(command),
    );
}

fn confirm_hard_update(
    parent: &ApplicationWindow,
    button: &Button,
    version_ref: Option<String>,
) {
    let dialog = MessageDialog::builder()
        .transient_for(parent)
        .message_type(gtk::MessageType::Warning)
        .buttons(gtk::ButtonsType::YesNo)
        .title("Confirm Hard Update")
        .text("Hard Update deletes the local application checkout, clones it again from GitHub, rebuilds it, and restarts the GUI. Local changes in that checkout will be lost. Continue?")
        .modal(true)
        .build();

    let parent_for_action = parent.clone();
    let button_for_action = button.clone();
    dialog.connect_response(move |dialog, response| {
        dialog.close();
        if response == gtk::ResponseType::Yes {
            hard_update_software_from_github(
                &parent_for_action,
                &button_for_action,
                version_ref.as_deref(),
            );
        }
    });
    dialog.show();
}

fn confirm_hard_delete(parent: &ApplicationWindow, button: &Button) {
    let Some(repo_dir) = software_repo_dir() else {
        show_message_dialog(
            parent,
            gtk::MessageType::Warning,
            "Repository Not Found",
            "I could not find a verified local checkout of Better Hyprland GUI.",
        );
        return;
    };

    let dialog = MessageDialog::builder()
        .transient_for(parent)
        .message_type(gtk::MessageType::Warning)
        .buttons(gtk::ButtonsType::YesNo)
        .title("Confirm Hard Delete")
        .text(&format!(
            "This permanently deletes the Better Hyprland GUI checkout at {} and its launcher state. Your Hyprland and dotfiles configuration will not be deleted. Continue?",
            repo_dir.display()
        ))
        .modal(true)
        .build();

    let parent_for_action = parent.clone();
    let button_for_action = button.clone();
    dialog.connect_response(move |dialog, response| {
        dialog.close();
        if response == gtk::ResponseType::Yes {
            hard_delete_software_from_github(&parent_for_action, &button_for_action);
        }
    });
    dialog.show();
}

fn run_hyprland_command(
    parent: &ApplicationWindow,
    button: &Button,
    running_label: &str,
    command: Command,
    success_title: &str,
    success_message: &str,
    failure_title: &str,
) {
    run_background_task(
        parent,
        Some(button),
        running_label,
        success_title,
        success_message,
        failure_title,
        move || command_result(command),
    );
}

fn nix_flake_ref_for_hyprland(version_ref: Option<&str>) -> String {
    match version_ref.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value.contains('#') => value.to_string(),
        Some(value) => format!("{value}#hyprland"),
        None => "nixpkgs#hyprland".to_string(),
    }
}

fn install_hyprland_from_gui(
    parent: &ApplicationWindow,
    button: &Button,
    version_ref: Option<&str>,
) {
    let distro = distro_id();
    let command = match distro.as_str() {
        "arch" | "manjaro" | "endeavouros" | "athena" | "athenaos" => {
            let mut command = Command::new("pkexec");
            command.args(["pacman", "-Syu", "--needed", "--noconfirm", "hyprland"]);
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
            command
                .arg("profile")
                .arg("install")
                .arg(nix_flake_ref_for_hyprland(version_ref));
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
        button,
        "Installing Hyprland…",
        command,
        "Hyprland Installed",
        "Hyprland installation finished successfully. Log out and select the Hyprland session if needed.",
        "Hyprland Install Failed",
    );
}

fn update_hyprland_from_gui(
    parent: &ApplicationWindow,
    button: &Button,
    version_ref: Option<&str>,
) {
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
        button,
        "Updating Hyprland…",
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
    let _ = persist_file_profile_store(store);
}

fn persist_file_profile_store(store: &FileProfileStore) -> Result<(), String> {
    let path = file_profiles_state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(store)
        .map_err(|error| format!("Could not serialize dotfiles state: {error}"))?;
    fs::write(&path, content)
        .map_err(|error| format!("Could not save {}: {error}", path.display()))
}

fn default_file_install_path() -> String {
    Path::new(&env::var("HOME").unwrap_or_else(|_| ".".to_string()))
        .join("dotfiles")
        .to_string_lossy()
        .to_string()
}

fn normalize_repo_url(value: &str) -> String {
    let trimmed = value.trim();

    if let Some((_, rest)) = trimmed.split_once("](") {
        if trimmed.starts_with('[') && rest.ends_with(')') {
            return rest[..rest.len() - 1].trim().to_string();
        }
    }

    trimmed
        .trim_matches(|c| matches!(c, '<' | '>' | '"' | '\''))
        .to_string()
}

fn validate_repository_url(value: &str) -> Result<String, String> {
    let normalized = normalize_repo_url(value);
    if normalized.is_empty() {
        return Err("Paste a Git repository URL first.".to_string());
    }
    if normalized
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err("The repository URL contains spaces or invalid characters.".to_string());
    }

    let (host, path) = if let Some(rest) = normalized.strip_prefix("git@") {
        rest.split_once(':')
            .map(|(host, path)| (host, path))
            .ok_or_else(|| {
                "Use a complete Git URL, for example git@github.com:user/dotfiles.git.".to_string()
            })?
    } else if let Some(rest) = normalized
        .strip_prefix("https://")
        .or_else(|| normalized.strip_prefix("http://"))
        .or_else(|| normalized.strip_prefix("ssh://"))
        .or_else(|| normalized.strip_prefix("git://"))
    {
        rest.split_once('/')
            .map(|(host, path)| (host, path))
            .ok_or_else(|| "Use a complete Git URL with a host and repository path.".to_string())?
    } else {
        return Err("Unsupported repository URL. Use HTTPS or SSH, for example https://github.com/user/dotfiles.git.".to_string());
    };

    let path_parts = path
        .trim_end_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if host.is_empty() || path_parts.len() < 2 || path_parts.iter().any(|part| *part == "..") {
        return Err("That link does not point to a Git repository. Use the repository's Clone URL from GitHub.".to_string());
    }
    if host.eq_ignore_ascii_case("youtube.com")
        || host.eq_ignore_ascii_case("www.youtube.com")
        || host.eq_ignore_ascii_case("youtu.be")
    {
        return Err("That is a video/redirect link, not a Git repository. Paste the dotfiles repository's Clone URL.".to_string());
    }

    Ok(normalized)
}

fn verify_repository_access(repo_url: &str) -> Result<(), String> {
    let output = git_command()
        .args(["ls-remote", "--quiet", "--", repo_url])
        .output()
        .map_err(|error| format!("Could not start Git: {error}"))?;
    if output.status.success() {
        return Ok(());
    }

    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if detail.is_empty() {
        "Git could not reach this repository. Check the URL and that the repository is public or accessible with your Git credentials.".to_string()
    } else {
        format!("Git could not reach this repository.\n\n{detail}")
    })
}

fn normalize_git_remote_identity(value: &str) -> String {
    let mut normalized = normalize_repo_url(value).replace('\\', "/");

    // Repository links copied from redirect pages can leave the wrapper URL
    // configured as origin. Compare the embedded GitHub repository instead.
    if let Some(start) = normalized.to_ascii_lowercase().find("github.com/") {
        normalized = normalized[start..].to_string();
    }
    normalized = normalized
        .split(&['&', '?', '#', ' ', '\n', '\r', ')', '>'][..])
        .next()
        .unwrap_or(&normalized)
        .to_string();

    if let Some(rest) = normalized.strip_prefix("git@") {
        normalized = rest.replacen(':', "/", 1);
    } else {
        for prefix in ["https://", "http://", "ssh://", "git://"] {
            if let Some(rest) = normalized.strip_prefix(prefix) {
                normalized = rest.to_string();
                break;
            }
        }
        normalized = normalized.trim_start_matches("git@").to_string();
    }

    normalized
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_lowercase()
}

fn verify_repo_remote(repo_dir: &Path, expected_url: &str) -> Result<(), String> {
    let output = git_command()
        .arg("-C")
        .arg(repo_dir)
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|err| format!("Failed to inspect the repository origin: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "The existing repository has no readable origin remote.\n\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let actual_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let actual_lower = actual_url.to_ascii_lowercase();
    if actual_lower.contains("youtube.com/redirect") || actual_lower.contains("youtu.be/") {
        // Older profiles could be created from a video-description redirect.
        // The selected profile URL is now authoritative, so repair the origin
        // before fetching instead of treating the existing checkout as foreign.
        let mut command = git_command();
        command
            .arg("-C")
            .arg(repo_dir)
            .args(["remote", "set-url", "origin", expected_url]);
        command_result(command)?;
        return Ok(());
    }

    let expected = normalize_git_remote_identity(expected_url);
    let actual = normalize_git_remote_identity(&actual_url);
    if expected.is_empty() || expected != actual {
        return Err(format!(
            "The install path belongs to a different repository.\n\nExpected: {expected_url}\nActual: {}",
            actual_url
        ));
    }

    Ok(())
}

fn expand_user_path(value: &str) -> PathBuf {
    let trimmed = value.trim();

    if trimmed == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from(trimmed));
    }

    if let Some(rest) = trimmed.strip_prefix("~/") {
        return home_dir()
            .map(|path| path.join(rest))
            .unwrap_or_else(|| PathBuf::from(trimmed));
    }

    PathBuf::from(trimmed)
}

fn file_profile_install_path(profile: &FileProfile) -> PathBuf {
    if profile.install_path.trim().is_empty() {
        expand_user_path(&default_file_install_path())
    } else {
        expand_user_path(profile.install_path.trim())
    }
}

fn file_profile_is_installed(profile: &FileProfile) -> bool {
    file_profile_install_path(profile).join(".git").is_dir()
}

fn file_profile_is_active(profile: &FileProfile) -> bool {
    load_file_profile_store().active_profile.as_deref() == Some(profile.name.as_str())
}

fn file_profile_status(profile: &FileProfile) -> &'static str {
    if file_profile_is_active(profile) {
        "Active"
    } else if file_profile_is_installed(profile) {
        "Installed"
    } else {
        "Not installed"
    }
}

fn file_profile_preview(profile: &FileProfile) -> String {
    let root = file_profile_install_path(profile);
    let candidates = [
        root.join(".config").join("hypr").join("hyprland.conf"),
        root.join("hyprland.conf"),
        root.join("README.md"),
        root.join("README"),
    ];

    for candidate in candidates {
        if let Ok(content) = fs::read_to_string(candidate) {
            let preview = content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .take(12)
                .collect::<Vec<_>>()
                .join("\n");
            if !preview.is_empty() {
                return preview;
            }
        }
    }

    if root.is_dir() {
        let mut entries = fs::read_dir(root)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .take(10)
            .collect::<Vec<_>>();
        entries.sort();
        if !entries.is_empty() {
            return entries
                .into_iter()
                .map(|entry| format!("./{entry}"))
                .collect::<Vec<_>>()
                .join("\n");
        }
    }

    "Install this profile to preview its files here.".to_string()
}

const HOME_LAYOUT_ENTRIES: &[&str] = &[
    ".config",
    ".local",
    ".bashrc",
    ".bash_profile",
    ".bash_logout",
    ".profile",
    ".zshrc",
    ".zprofile",
    ".zshenv",
    ".tmux.conf",
    ".vimrc",
    ".Xresources",
    ".xinitrc",
    ".gtkrc-2.0",
];

fn xdg_config_home(home: &Path) -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
}

fn has_home_layout_entries(root: &Path) -> bool {
    HOME_LAYOUT_ENTRIES
        .iter()
        .any(|name| root.join(name).exists())
}

fn add_home_layout_plan(
    source_root: &Path,
    home: &Path,
    plan: &mut Vec<(PathBuf, PathBuf)>,
) {
    for name in HOME_LAYOUT_ENTRIES {
        let source = source_root.join(name);
        if !source.exists() {
            continue;
        }

        let destination = home.join(name);
        if !plan
            .iter()
            .any(|(existing_source, existing_destination)| {
                existing_source == &source && existing_destination == &destination
            })
        {
            plan.push((source, destination));
        }
    }
}

fn add_hypr_layout_plan(source_root: &Path, home: &Path, plan: &mut Vec<(PathBuf, PathBuf)>) {
    let source = source_root.join("hypr");
    if source.is_dir() {
        let destination = xdg_config_home(home).join("hypr");
        if !plan
            .iter()
            .any(|(existing_source, existing_destination)| {
                existing_source == &source && existing_destination == &destination
            })
        {
            plan.push((source, destination));
        }
    }

    let source = source_root.join("hyprland.conf");
    if source.is_file() {
        plan.push((
            source,
            xdg_config_home(home).join("hypr").join("hyprland.conf"),
        ));
    }
}

fn profile_copy_plan(
    profile_root: &Path,
    home: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let config_home = xdg_config_home(home);
    if !config_home.is_absolute() || !config_home.starts_with(home) {
        return Err(
            "XDG_CONFIG_HOME must be an absolute path inside the home directory for safe activation."
                .to_string(),
        );
    }

    let mut plan = Vec::new();

    // Home-tree layouts used by chezmoi-like repositories and repositories
    // such as end-4/dots-hyprland.
    for layout_name in ["dots", "home", "dotfiles"] {
        let source_root = profile_root.join(layout_name);
        if source_root.is_dir() {
            add_home_layout_plan(&source_root, home, &mut plan);
            add_hypr_layout_plan(&source_root, home, &mut plan);
        }
    }

    // A repository can mirror $HOME directly, or expose an XDG config tree.
    add_home_layout_plan(profile_root, home, &mut plan);
    add_hypr_layout_plan(profile_root, home, &mut plan);

    let config_root = profile_root.join("config");
    if config_root.join("hypr").is_dir() {
        plan.push((config_root, xdg_config_home(home)));
    }

    // GNU Stow-style repositories contain packages such as `hyprland/.config`
    // or `nvim/.config`. Only directories that look like home trees qualify;
    // README, scripts, and metadata are never copied.
    if plan.is_empty() {
        for entry in fs::read_dir(profile_root)
            .map_err(|error| format!("Could not read {}: {error}", profile_root.display()))?
        {
            let entry = entry.map_err(|error| format!("Could not inspect profile: {error}"))?;
            let source_root = entry.path();
            if source_root.is_dir()
                && entry.file_name() != ".git"
                && has_home_layout_entries(&source_root)
            {
                add_home_layout_plan(&source_root, home, &mut plan);
                add_hypr_layout_plan(&source_root, home, &mut plan);
            }
        }
    }

    if plan.is_empty() {
        return Err("This profile does not contain a supported dotfiles tree. Expected dots/home/dotfiles, a .config tree, a hypr directory, a root home layout, or GNU Stow-style packages.".to_string());
    }

    Ok(plan)
}

fn ensure_destination_parents_are_not_symlinks(destination: &Path) -> Result<(), String> {
    let mut current = destination.parent();
    while let Some(path) = current {
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "Refusing to write through symlinked destination directory {}.",
                    path.display()
                ));
            }
        }

        let Some(parent) = path.parent() else {
            break;
        };
        if parent == path {
            break;
        }
        current = Some(parent);
    }
    Ok(())
}

fn copy_profile_tree(source: &Path, destination: &Path) -> Result<(), String> {
    ensure_destination_parents_are_not_symlinks(destination)?;
    let source_metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("Could not inspect {}: {error}", source.display()))?;
    if source_metadata.file_type().is_symlink() {
        return Err(format!(
            "Refusing to copy symlinked profile path {}.",
            source.display()
        ));
    }

    if source_metadata.is_dir() {
        if let Ok(destination_metadata) = fs::symlink_metadata(destination) {
            if destination_metadata.file_type().is_symlink() {
                return Err(format!(
                    "Refusing to copy into symlinked destination {}. Remove it or choose another profile path first.",
                    destination.display()
                ));
            }
            if !destination_metadata.is_dir() {
                fs::remove_file(destination).map_err(|error| {
                    format!("Could not replace {} with a directory: {error}", destination.display())
                })?;
            }
        }
        fs::create_dir_all(destination)
            .map_err(|error| format!("Could not create {}: {error}", destination.display()))?;
        for entry in fs::read_dir(source)
            .map_err(|error| format!("Could not read {}: {error}", source.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Could not inspect profile file: {error}"))?;
            if entry.file_name() == ".git" {
                continue;
            }
            copy_profile_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }

    if let Ok(destination_metadata) = fs::symlink_metadata(destination) {
        if destination_metadata.file_type().is_symlink() {
            return Err(format!(
                "Refusing to replace symlinked destination {}. Remove it or choose another profile path first.",
                destination.display()
            ));
        }
        if destination_metadata.is_dir() {
            fs::remove_dir_all(destination)
                .map_err(|error| format!("Could not replace {}: {error}", destination.display()))?;
        } else {
            fs::remove_file(destination)
                .map_err(|error| format!("Could not replace {}: {error}", destination.display()))?;
        }
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    fs::copy(source, destination)
        .map_err(|error| format!("Could not copy {}: {error}", source.display()))?;
    Ok(())
}

fn resolve_profile_source(profile_root: &Path, source: &Path) -> Result<PathBuf, String> {
    let resolved = fs::canonicalize(source)
        .map_err(|error| format!("Could not resolve profile path {}: {error}", source.display()))?;
    if !resolved.starts_with(profile_root) {
        return Err(format!(
            "Refusing to activate symlinked profile path {} because it points outside the installed profile.",
            source.display()
        ));
    }
    Ok(resolved)
}

fn copy_profile_tree_from_profile(
    source: &Path,
    destination: &Path,
    profile_root: &Path,
) -> Result<(), String> {
    let mut active_directories = Vec::new();
    copy_profile_tree_from_profile_inner(
        source,
        destination,
        profile_root,
        &mut active_directories,
    )
}

fn copy_profile_tree_from_profile_inner(
    source: &Path,
    destination: &Path,
    profile_root: &Path,
    active_directories: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let resolved_source = resolve_profile_source(profile_root, source)?;
    let metadata = fs::metadata(&resolved_source)
        .map_err(|error| format!("Could not inspect {}: {error}", source.display()))?;

    ensure_destination_parents_are_not_symlinks(destination)?;
    if metadata.is_dir() {
        if active_directories.iter().any(|path| path == &resolved_source) {
            return Err(format!(
                "Refusing to activate cyclic profile directory {}.",
                source.display()
            ));
        }
        active_directories.push(resolved_source.clone());

        if let Ok(destination_metadata) = fs::symlink_metadata(destination) {
            if destination_metadata.file_type().is_symlink() {
                return Err(format!(
                    "Refusing to copy into symlinked destination {}. Remove it or choose another profile path first.",
                    destination.display()
                ));
            }
            if !destination_metadata.is_dir() {
                fs::remove_file(destination).map_err(|error| {
                    format!("Could not replace {} with a directory: {error}", destination.display())
                })?;
            }
        }
        fs::create_dir_all(destination)
            .map_err(|error| format!("Could not create {}: {error}", destination.display()))?;
        for entry in fs::read_dir(&resolved_source)
            .map_err(|error| format!("Could not read {}: {error}", source.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Could not inspect profile file: {error}"))?;
            if entry.file_name() != ".git" {
                copy_profile_tree_from_profile_inner(
                    &entry.path(),
                    &destination.join(entry.file_name()),
                    profile_root,
                    active_directories,
                )?;
            }
        }
        active_directories.pop();
        return Ok(());
    }

    if let Ok(destination_metadata) = fs::symlink_metadata(destination) {
        if destination_metadata.file_type().is_symlink() {
            return Err(format!(
                "Refusing to replace symlinked destination {}. Remove it or choose another profile path first.",
                destination.display()
            ));
        }
        if destination_metadata.is_dir() {
            fs::remove_dir_all(destination)
                .map_err(|error| format!("Could not replace {}: {error}", destination.display()))?;
        } else {
            fs::remove_file(destination)
                .map_err(|error| format!("Could not replace {}: {error}", destination.display()))?;
        }
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    fs::copy(&resolved_source, destination)
        .map_err(|error| format!("Could not copy {}: {error}", source.display()))?;
    Ok(())
}

fn collect_profile_files(
    source: &Path,
    destination: &Path,
    home: &Path,
    profile_root: &Path,
    entries: &mut Vec<String>,
) -> Result<(), String> {
    let mut active_directories = Vec::new();
    collect_profile_files_inner(
        source,
        destination,
        home,
        profile_root,
        entries,
        &mut active_directories,
    )
}

fn collect_profile_files_inner(
    source: &Path,
    destination: &Path,
    home: &Path,
    profile_root: &Path,
    entries: &mut Vec<String>,
    active_directories: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let resolved_source = resolve_profile_source(profile_root, source)?;
    let metadata = fs::metadata(&resolved_source)
        .map_err(|error| format!("Could not inspect {}: {error}", source.display()))?;
    if metadata.is_dir() {
        if active_directories.iter().any(|path| path == &resolved_source) {
            return Err(format!(
                "Refusing to activate cyclic profile directory {}.",
                source.display()
            ));
        }
        active_directories.push(resolved_source.clone());
        for entry in fs::read_dir(&resolved_source)
            .map_err(|error| format!("Could not read {}: {error}", source.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Could not inspect profile file: {error}"))?;
            if entry.file_name() != ".git" {
                collect_profile_files_inner(
                    &entry.path(),
                    &destination.join(entry.file_name()),
                    home,
                    profile_root,
                    entries,
                    active_directories,
                )?;
            }
        }
        active_directories.pop();
        return Ok(());
    }

    let relative = destination
        .strip_prefix(home)
        .map_err(|_| format!("Profile destination escapes home: {}", destination.display()))?
        .to_string_lossy()
        .replace('\\', "/");
    if entries.iter().any(|entry| entry == &relative) {
        return Err(format!("Multiple profile sources target {relative}."));
    }
    entries.push(relative);
    Ok(())
}

fn safe_home_path(home: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return Err(format!("Refusing unsafe managed path: {relative}"));
    }

    let path = home.join(relative_path);
    if !path.starts_with(home) {
        return Err(format!("Managed path escapes home: {relative}"));
    }
    Ok(path)
}

fn sanitized_profile_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "profile".to_string()
    } else {
        sanitized
    }
}

fn apply_file_profile(profile: &FileProfile) -> Result<String, String> {
    let profile_root = file_profile_install_path(profile);
    if !profile_root.join(".git").is_dir() {
        return Err("Install this profile before applying it.".to_string());
    }
    let canonical_profile_root = fs::canonicalize(&profile_root)
        .map_err(|error| format!("Could not resolve installed profile path: {error}"))?;

    let home = home_dir().ok_or_else(|| "Could not determine the home directory.".to_string())?;
    let copy_plan = profile_copy_plan(&profile_root, &home)?;
    for (source, destination) in &copy_plan {
        if source == destination || source.starts_with(destination) || destination.starts_with(source) {
            return Err(format!(
                "Profile source {} overlaps its activation destination {}.",
                source.display(),
                destination.display()
            ));
        }
    }

    let mut manifest = Vec::new();
    for (source, destination) in &copy_plan {
        collect_profile_files(
            source,
            destination,
            &home,
            &canonical_profile_root,
            &mut manifest,
        )?;
    }
    if manifest.is_empty() {
        return Err("The selected profile does not contain any files that can be activated.".to_string());
    }

    let store = load_file_profile_store();
    let stamp = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("Could not create backup timestamp: {error}"))?
        .as_secs();
    let backup = home
        .join(".config")
        .join("hyprgui")
        .join("backups")
        .join(format!("{}-{stamp}", sanitized_profile_name(&profile.name)));

    let mut backup_entries = store.active_entries.clone();
    backup_entries.extend(manifest.iter().cloned());
    backup_entries.sort();
    backup_entries.dedup();
    for relative in &backup_entries {
        let current = safe_home_path(&home, relative)?;
        if fs::symlink_metadata(&current).is_ok() {
            copy_profile_tree(&current, &backup.join(relative))?;
        }
    }

    for relative in &store.active_entries {
        let current = safe_home_path(&home, relative)?;
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() || metadata.is_file() {
                fs::remove_file(&current)
                    .map_err(|error| format!("Could not remove previous managed file {}: {error}", current.display()))?;
            } else {
                return Err(format!(
                    "Previous managed path is a directory; refusing to remove {}.",
                    current.display()
                ));
            }
        }
    }

    for (source, destination) in &copy_plan {
        copy_profile_tree_from_profile(source, destination, &canonical_profile_root)?;
    }

    let mut next_store = store;
    next_store.active_profile = Some(profile.name.clone());
    next_store.active_entries = manifest;
    persist_file_profile_store(&next_store)?;

    let reload = Command::new("hyprctl").arg("reload").output();
    let message = if reload
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        format!("Profile '{}' applied and Hyprland reloaded.", profile.name)
    } else {
        format!(
            "Profile '{}' applied. Reload Hyprland manually if the changes are not visible.",
            profile.name
        )
    };
    Ok(message)
}

fn file_profile_name_from_url(url: &str) -> String {
    normalize_repo_url(url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("Dotfiles")
        .trim_end_matches(".git")
        .to_string()
}

fn install_file_profile(
    parent: &ApplicationWindow,
    button: &Button,
    profile: &FileProfile,
    on_success: Option<std::boxed::Box<dyn FnOnce() + 'static>>,
) {
    let mut on_success = on_success;
    let repo_url = match validate_repository_url(&profile.repo_url) {
        Ok(url) => url,
        Err(error) => {
            show_message_dialog(
                parent,
                gtk::MessageType::Warning,
                "Invalid Repository",
                &error,
            );
            return;
        }
    };
    if repo_url.is_empty() {
        show_message_dialog(
            parent,
            gtk::MessageType::Warning,
            "Missing Repo",
            "The selected .file profile does not contain a GitHub repository URL.",
        );
        return;
    }

    let install_path = file_profile_install_path(profile)
        .to_string_lossy()
        .to_string();

    let target_path = PathBuf::from(&install_path);
    let version_ref = profile.version_ref.trim().to_string();

    if target_path.exists() && !target_path.join(".git").exists() {
        let is_empty_dir = target_path.is_dir()
            && fs::read_dir(&target_path)
                .ok()
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
        if !is_empty_dir {
            show_message_dialog(
                parent,
                gtk::MessageType::Warning,
                "Existing Folder",
                "The install path already exists, but it is not an empty folder or Git repository. Pick a different path.",
            );
            return;
        }
    }

    if target_path.join(".git").exists() {
        run_background_task_with_completion(
            parent,
            Some(button),
            "Updating dotfiles…",
            "Dotfiles Updated",
            "The selected .file profile was updated successfully.",
            "Dotfiles Update Failed",
            on_success.take(),
            move || {
                verify_repo_remote(&target_path, &repo_url)?;
                ensure_repo_clean(&target_path)?;
                fetch_repo(&target_path)?;

                if !version_ref.is_empty() {
                    return checkout_repo_ref(&target_path, &version_ref);
                }

                let branch = git_current_branch(&target_path)?
                    .filter(|branch| branch != "HEAD")
                    .ok_or_else(|| {
                        "The repository is in detached-HEAD state. Select an explicit version ref before updating."
                            .to_string()
                    })?;
                let remote_branch = format!("origin/{branch}");
                let mut command = git_command();
                command
                    .arg("-C")
                    .arg(&target_path)
                    .args(["merge", "--ff-only", &remote_branch]);
                command_result(command)
            },
        );
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

    let mut command = git_command();
    command.arg("clone");
    if !version_ref.is_empty() {
        if let Err(error) = validate_version_ref(&version_ref) {
            show_message_dialog(
                parent,
                gtk::MessageType::Warning,
                "Invalid Version Ref",
                &error,
            );
            return;
        }
        command.args(["--branch", &version_ref]);
    }
    command.arg("--").arg(&repo_url).arg(&install_path);

    run_background_task_with_completion(
        parent,
        Some(button),
        "Cloning dotfiles…",
        "Dotfiles Installed",
        "The selected .file profile was installed successfully.",
        "Dotfiles Install Failed",
        on_success,
        move || {
            verify_repository_access(&repo_url)?;
            command_result(command)
        },
    );
}

fn button_with_icon_label(icon_name: &str, text: &str) -> Button {
    let button = Button::new();
    let inner = Box::new(Orientation::Horizontal, 6);
    let icon = icon_image(icon_name);
    let label = Label::new(Some(text));
    inner.append(&icon);
    inner.append(&label);
    button.set_child(Some(&inner));
    button
}

fn install_wallpaper_engine_css() {
    let provider = CssProvider::new();
    provider.load_from_data(
        ".wallpaper-page { background-color: #17121f; }
         .wallpaper-gallery, .wallpaper-details { border-radius: 10px; }
         .wallpaper-card { border-radius: 10px; padding: 2px; }
         .wallpaper-card-selected { border: 2px solid #e8a1b0; background-color: #8d536f; }
         .wallpaper-chip { border-radius: 999px; padding-left: 14px; padding-right: 14px; }
         .wallpaper-action { min-height: 42px; border-radius: 8px; }
         .wallpaper-search { border-radius: 8px; }
         .welcome-page { background-color: #17121f; }
         .welcome-hero { border-radius: 14px; background-color: #2b1d32; border: 1px solid #67486f; }
         .welcome-card { border-radius: 10px; background-color: #211827; border: 1px solid #3e2c47; }
         .welcome-action { min-height: 42px; border-radius: 8px; }",
    );
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
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
            body: "Paste a GitHub link for your dotfiles, open it directly, or clone it in the background to start the setup flow.",
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
        step_label_back.set_markup(&format!(
            "<span size=\"large\"><b>{}</b></span>",
            step.title
        ));
        target_label_back.set_markup(&format!("<b>Spotlight:</b> {}", step.target));
        body_label_back.set_text(step.body);
        tip_label_back.set_text(step.tip);
        back_button_back.set_sensitive(index > 0);
        next_button_back.set_sensitive(true);
        next_button_back.set_label(if index + 1 < steps_back.len() {
            "Next"
        } else {
            "Finish"
        });
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

type RefreshCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub struct ConfigGUI {
    pub window: ApplicationWindow,
    config_widgets: HashMap<String, ConfigWidget>,
    pub save_button: Button,
    content_box: Box,
    changed_options: Rc<RefCell<HashMap<(String, String), String>>>,
    file_profiles: Rc<RefCell<FileProfileStore>>,
    file_profiles_refresh: RefreshCallback,
    stack: Stack,
    sidebar: StackSidebar,
    load_config_button: Button,
    save_config_button: Button,
    pub gear_menu: Rc<RefCell<Popover>>,
}

impl ConfigGUI {
    pub fn new(app: &Application) -> Self {
        install_wallpaper_engine_css();
        let window = ApplicationWindow::builder()
            .application(app)
            .default_width(1280)
            .default_height(760)
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
        let question_mark_icon = icon_image("dialog-question-symbolic");
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
        main_box.set_hexpand(true);
        main_box.set_vexpand(true);

        let content_box = Box::new(Orientation::Horizontal, 0);
        content_box.set_hexpand(true);
        content_box.set_vexpand(true);
        main_box.append(&content_box);

        window.set_child(Some(&main_box));

        let config_widgets = HashMap::new();

        let stack = Stack::new();
        stack.set_hexpand(true);
        stack.set_vexpand(true);
        stack.set_size_request(0, -1);

        let sidebar = StackSidebar::new();
        sidebar.set_stack(&stack);
        sidebar.set_hexpand(false);
        sidebar.set_vexpand(true);
        sidebar.set_size_request(200, -1);

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
        self.sidebar.set_hexpand(false);
        self.sidebar.set_vexpand(true);
        self.sidebar.set_size_request(200, -1);
        self.stack.set_hexpand(true);
        self.stack.set_vexpand(true);
        self.stack.set_size_request(0, -1);

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
        container.add_css_class("welcome-page");
        container.set_margin_top(16);
        container.set_margin_bottom(16);
        container.set_margin_start(16);
        container.set_margin_end(16);

        let hero = Frame::new(None);
        hero.add_css_class("welcome-hero");
        let hero_box = Box::new(Orientation::Vertical, 8);
        hero_box.set_margin_top(22);
        hero_box.set_margin_bottom(22);
        hero_box.set_margin_start(22);
        hero_box.set_margin_end(22);

        let welcome_label = Label::new(Some("Welcome to HyprGUI"));
        welcome_label.set_markup("<span size=\"xx-large\"><b>Welcome to HyprGUI</b></span>");
        welcome_label.set_halign(gtk::Align::Start);

        let intro_label = Label::new(Some(
            "A calm starting point for building your Hyprland desktop. Pick a direction and make it yours.",
        ));
        intro_label.set_wrap(true);
        intro_label.set_halign(gtk::Align::Start);
        intro_label.set_opacity(0.86);

        let status_label = Label::new(Some(note));
        status_label.set_wrap(true);
        status_label.set_halign(gtk::Align::Start);
        status_label.set_opacity(0.72);
        hero_box.append(&welcome_label);
        hero_box.append(&intro_label);
        hero_box.append(&status_label);
        hero.set_child(Some(&hero_box));

        let title_label = Label::new(Some("Your setup"));
        title_label.set_markup("<b>Your setup</b>");
        title_label.set_halign(gtk::Align::Start);

        let note_label = Label::new(Some(note));
        note_label.set_wrap(true);
        note_label.set_halign(gtk::Align::Start);

        let help_label = Label::new(Some(
            "Everything you need is in the sidebar. Start with a dotfile collection or install Hyprland first.",
        ));
        help_label.set_wrap(true);
        help_label.set_opacity(0.8);
        help_label.set_halign(gtk::Align::Start);

        let cards = Box::new(Orientation::Horizontal, 12);
        cards.set_homogeneous(true);
        for (heading, body) in [
            (
                "Dotfiles",
                "Browse GitHub profiles, install them, and switch between your favourites.",
            ),
            (
                "Hyprland",
                "Install or update Hyprland and keep your version choices in one place.",
            ),
            (
                "Configure",
                "Fine-tune your desktop and save your changes when everything feels right.",
            ),
        ] {
            let card = Frame::new(None);
            card.add_css_class("welcome-card");
            let card_box = Box::new(Orientation::Vertical, 6);
            card_box.set_margin_top(14);
            card_box.set_margin_bottom(14);
            card_box.set_margin_start(14);
            card_box.set_margin_end(14);
            let card_title = Label::new(Some(heading));
            card_title.set_markup(&format!("<b>{heading}</b>"));
            card_title.set_halign(gtk::Align::Start);
            let card_body = Label::new(Some(body));
            card_body.set_wrap(true);
            card_body.set_halign(gtk::Align::Start);
            card_box.append(&card_title);
            card_box.append(&card_body);
            card.set_child(Some(&card_box));
            cards.append(&card);
        }

        let action_row = Box::new(Orientation::Horizontal, 10);
        let dotfiles_button = Button::with_label("Browse dotfiles");
        dotfiles_button.add_css_class("welcome-action");
        let install_button = Button::with_label("Open Hyprland setup");
        install_button.add_css_class("welcome-action");
        action_row.append(&dotfiles_button);
        action_row.append(&install_button);

        let stack_for_dotfiles = self.stack.clone();
        dotfiles_button.connect_clicked(move |_| {
            stack_for_dotfiles.set_visible_child_name("files");
        });
        let stack_for_install = self.stack.clone();
        install_button.connect_clicked(move |_| {
            stack_for_install.set_visible_child_name("hyprland-install");
        });

        container.append(&hero);
        container.append(&title_label);
        container.append(&help_label);
        container.append(&cards);
        container.append(&action_row);
        container.append(&note_label);

        scrolled_window.set_child(Some(&container));
        self.stack
            .add_titled(&scrolled_window, Some("setup"), "Setup");
    }

    fn add_files_page(&mut self) {
        let scrolled_window = ScrolledWindow::new();
        scrolled_window.set_vexpand(true);
        scrolled_window.set_hexpand(true);

        let container = Box::new(Orientation::Vertical, 12);
        container.add_css_class("wallpaper-page");
        container.set_margin_top(16);
        container.set_margin_bottom(16);
        container.set_margin_start(16);
        container.set_margin_end(16);

        let header_row = Box::new(Orientation::Horizontal, 10);
        let title_box = Box::new(Orientation::Vertical, 3);
        title_box.set_hexpand(true);
        let title_label = Label::new(Some(".files"));
        title_label.set_markup("<b>.files</b>");
        title_label.set_halign(gtk::Align::Start);
        let description_label = Label::new(Some(
            "Browse, preview, and install dotfiles from Git repositories in one workspace.",
        ));
        description_label.set_halign(gtk::Align::Start);
        description_label.set_wrap(true);
        description_label.set_hexpand(true);
        description_label.set_opacity(0.75);
        title_box.append(&title_label);
        title_box.append(&description_label);

        let add_profile_button = button_with_icon_label("list-add-symbolic", "Add .file");
        add_profile_button.set_tooltip_text(Some("Save a repository as a reusable .file profile"));
        let refresh_button = Button::from_icon_name("view-refresh-symbolic");
        refresh_button.set_tooltip_text(Some("Reload saved profiles and installed status"));
        header_row.append(&title_box);
        let header_spacer = Box::new(Orientation::Horizontal, 0);
        header_spacer.set_hexpand(true);
        header_row.append(&header_spacer);
        header_row.append(&add_profile_button);
        header_row.append(&refresh_button);

        let search_row = Box::new(Orientation::Horizontal, 8);
        let search_entry = Entry::new();
        search_entry.add_css_class("wallpaper-search");
        search_entry.set_hexpand(true);
        search_entry.set_placeholder_text(Some("Search .files by name or repository"));
        search_entry.set_icon_from_icon_name(
            gtk::EntryIconPosition::Primary,
            Some("system-search-symbolic"),
        );
        let clear_search_button = Button::from_icon_name("edit-clear-symbolic");
        clear_search_button.set_tooltip_text(Some("Clear search"));
        let sort_model = gtk::StringList::new(&["Selected first", "Name", "Repository"]);
        let sort_dropdown = DropDown::new(Some(sort_model), None::<gtk::Expression>);
        sort_dropdown.set_selected(0);
        sort_dropdown.set_tooltip_text(Some("Change profile order"));
        search_row.append(&search_entry);
        search_row.append(&clear_search_button);
        search_row.append(&sort_dropdown);

        let filter_row = Box::new(Orientation::Horizontal, 6);
        let filter_label = Label::new(Some("Show:"));
        filter_label.set_opacity(0.75);
        let all_filter_button = Button::with_label("All");
        let installed_filter_button = Button::with_label("Installed");
        let missing_filter_button = Button::with_label("Not installed");
        all_filter_button.add_css_class("wallpaper-chip");
        installed_filter_button.add_css_class("wallpaper-chip");
        missing_filter_button.add_css_class("wallpaper-chip");
        all_filter_button.set_hexpand(true);
        installed_filter_button.set_hexpand(true);
        missing_filter_button.set_hexpand(true);
        filter_row.append(&filter_label);
        filter_row.append(&all_filter_button);
        filter_row.append(&installed_filter_button);
        filter_row.append(&missing_filter_button);

        let body_row = Paned::new(Orientation::Horizontal);
        body_row.set_hexpand(true);
        body_row.set_vexpand(true);
        body_row.set_position(360);
        body_row.set_wide_handle(true);

        let gallery_frame = Frame::new(Some("Dotfile profiles"));
        gallery_frame.add_css_class("wallpaper-gallery");
        gallery_frame.set_hexpand(true);
        gallery_frame.set_vexpand(true);
        gallery_frame.set_size_request(260, -1);
        let gallery_scroller = ScrolledWindow::new();
        gallery_scroller.set_vexpand(true);
        gallery_scroller.set_hexpand(true);
        let gallery_grid = Grid::new();
        gallery_grid.set_column_spacing(10);
        gallery_grid.set_row_spacing(10);
        gallery_grid.set_margin_top(10);
        gallery_grid.set_margin_bottom(10);
        gallery_grid.set_margin_start(10);
        gallery_grid.set_margin_end(10);
        gallery_scroller.set_child(Some(&gallery_grid));
        gallery_frame.set_child(Some(&gallery_scroller));

        let detail_frame = Frame::new(Some("Profile details"));
        detail_frame.add_css_class("wallpaper-details");
        detail_frame.set_hexpand(true);
        detail_frame.set_vexpand(true);
        let detail_scroller = ScrolledWindow::new();
        detail_scroller.set_hexpand(true);
        detail_scroller.set_vexpand(true);
        detail_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        let detail_box = Box::new(Orientation::Vertical, 9);
        detail_box.set_margin_top(12);
        detail_box.set_margin_bottom(12);
        detail_box.set_margin_start(12);
        detail_box.set_margin_end(12);

        let preview_heading = Label::new(Some("Preview"));
        preview_heading.set_halign(gtk::Align::Start);
        preview_heading.set_opacity(0.75);
        let preview_title = Label::new(Some("No .file selected"));
        preview_title.set_markup("<b>No .file selected</b>");
        preview_title.set_halign(gtk::Align::Start);
        let preview_label = Label::new(Some(
            "Select a profile to inspect its files and install target.",
        ));
        preview_label.set_halign(gtk::Align::Start);
        preview_label.set_wrap(true);
        preview_label.set_selectable(true);
        preview_label.set_xalign(0.0);
        preview_label.set_yalign(0.0);
        preview_label.set_size_request(-1, 96);

        let status_label = Label::new(Some("Status: -"));
        status_label.set_halign(gtk::Align::Start);
        let repo_label = Label::new(Some("Repository: -"));
        repo_label.set_halign(gtk::Align::Start);
        repo_label.set_wrap(true);
        repo_label.set_wrap_mode(gtk::pango::WrapMode::Char);
        repo_label.set_hexpand(true);
        repo_label.set_xalign(0.0);
        repo_label.set_max_width_chars(42);
        repo_label.set_selectable(true);
        let path_label = Label::new(Some("Install path: -"));
        path_label.set_halign(gtk::Align::Start);
        path_label.set_wrap(true);
        path_label.set_wrap_mode(gtk::pango::WrapMode::Char);
        path_label.set_hexpand(true);
        path_label.set_xalign(0.0);
        path_label.set_max_width_chars(42);
        let version_label = Label::new(Some("Branch / ref: latest"));
        version_label.set_halign(gtk::Align::Start);
        version_label.set_wrap(true);
        version_label.set_wrap_mode(gtk::pango::WrapMode::Char);
        version_label.set_hexpand(true);
        version_label.set_xalign(0.0);
        version_label.set_max_width_chars(42);
        let notes_label = Label::new(Some("Notes: -"));
        notes_label.set_halign(gtk::Align::Start);
        notes_label.set_wrap(true);
        notes_label.set_wrap_mode(gtk::pango::WrapMode::Char);
        notes_label.set_hexpand(true);
        notes_label.set_xalign(0.0);
        notes_label.set_max_width_chars(42);
        notes_label.set_opacity(0.8);

        let open_profile_button = Button::with_label("Open repository");
        open_profile_button
            .set_tooltip_text(Some("Open the selected repository in the default browser"));
        let apply_profile_button = Button::with_label("Apply profile");
        apply_profile_button.set_tooltip_text(Some(
            "Copy this profile's Hyprland files into the active configuration and reload Hyprland",
        ));
        apply_profile_button.add_css_class("suggested-action");
        let run_command_button = Button::with_label("Install / Update");
        run_command_button.set_tooltip_text(Some(
            "Clone a new profile or fast-forward an installed profile",
        ));
        run_command_button.add_css_class("suggested-action");
        let remove_profile_button = Button::with_label("Remove from library");
        remove_profile_button
            .set_tooltip_text(Some("Remove the saved profile without deleting its files"));
        open_profile_button.set_hexpand(true);
        apply_profile_button.set_hexpand(true);
        run_command_button.set_hexpand(true);
        remove_profile_button.set_hexpand(true);
        open_profile_button.add_css_class("wallpaper-action");
        apply_profile_button.add_css_class("wallpaper-action");
        run_command_button.add_css_class("wallpaper-action");
        remove_profile_button.add_css_class("wallpaper-action");

        detail_box.append(&preview_heading);
        detail_box.append(&preview_title);
        detail_box.append(&preview_label);
        detail_box.append(&Separator::new(Orientation::Horizontal));
        detail_box.append(&status_label);
        detail_box.append(&repo_label);
        detail_box.append(&path_label);
        detail_box.append(&version_label);
        detail_box.append(&notes_label);
        detail_box.append(&Separator::new(Orientation::Horizontal));
        detail_box.append(&open_profile_button);
        detail_box.append(&apply_profile_button);
        detail_box.append(&run_command_button);
        detail_box.append(&remove_profile_button);
        detail_scroller.set_child(Some(&detail_box));
        detail_frame.set_child(Some(&detail_scroller));

        body_row.set_start_child(Some(&gallery_frame));
        body_row.set_end_child(Some(&detail_frame));

        let quick_install_frame = Frame::new(Some("Install another repository"));
        let quick_install_box = Box::new(Orientation::Vertical, 8);
        quick_install_box.set_margin_top(10);
        quick_install_box.set_margin_bottom(10);
        quick_install_box.set_margin_start(10);
        quick_install_box.set_margin_end(10);
        let quick_repo_entry = Entry::new();
        quick_repo_entry.set_placeholder_text(Some("https://github.com/username/dotfiles"));
        let quick_path_entry = Entry::new();
        quick_path_entry.set_placeholder_text(Some(&default_file_install_path()));
        let quick_ref_entry = Entry::new();
        quick_ref_entry.set_placeholder_text(Some("Optional branch, tag, or commit"));
        let quick_install_button = Button::with_label("Install repository");
        quick_install_button.add_css_class("suggested-action");
        quick_install_box.append(&quick_repo_entry);
        quick_install_box.append(&quick_path_entry);
        quick_install_box.append(&quick_ref_entry);
        quick_install_box.append(&quick_install_button);
        quick_install_frame.set_child(Some(&quick_install_box));

        let store = self.file_profiles.clone();
        let initial_selected = store.borrow().selected.clone();
        let selected_name = Rc::new(RefCell::new(initial_selected));
        let filter_mode = Rc::new(RefCell::new("all".to_string()));
        let sort_mode = Rc::new(RefCell::new("recent".to_string()));

        let update_detail: Rc<dyn Fn(Option<FileProfile>)> = {
            let preview_title = preview_title.clone();
            let preview_label = preview_label.clone();
            let status_label = status_label.clone();
            let repo_label = repo_label.clone();
            let path_label = path_label.clone();
            let version_label = version_label.clone();
            let notes_label = notes_label.clone();
            let open_profile_button = open_profile_button.clone();
            let apply_profile_button = apply_profile_button.clone();
            let run_command_button = run_command_button.clone();
            let remove_profile_button = remove_profile_button.clone();
            Rc::new(move |profile| {
                let Some(profile) = profile else {
                    preview_title.set_markup("<b>No .file selected</b>");
                    preview_label
                        .set_text("Select a profile to inspect its files and install target.");
                    status_label.set_text("Status: -");
                    repo_label.set_text("Repository: -");
                    path_label.set_text("Install path: -");
                    version_label.set_text("Branch / ref: latest");
                    notes_label.set_text("Notes: -");
                    open_profile_button.set_sensitive(false);
                    apply_profile_button.set_sensitive(false);
                    run_command_button.set_sensitive(false);
                    remove_profile_button.set_sensitive(false);
                    return;
                };

                let escaped_name = glib::markup_escape_text(&profile.name);
                preview_title.set_markup(&format!("<b>{}</b>", escaped_name));
                preview_label.set_text(&file_profile_preview(&profile));
                status_label.set_text(&format!("Status: {}", file_profile_status(&profile)));
                repo_label.set_text(&format!("Repository: {}", profile.repo_url));
                path_label.set_text(&format!(
                    "Install path: {}",
                    file_profile_install_path(&profile).to_string_lossy()
                ));
                let version_text = if profile.version_ref.trim().is_empty() {
                    "Branch / ref: latest".to_string()
                } else {
                    format!("Branch / ref: {}", profile.version_ref)
                };
                version_label.set_text(&version_text);
                let notes_text = if profile.notes.trim().is_empty() {
                    "Notes: No extra notes saved.".to_string()
                } else {
                    format!("Notes: {}", profile.notes)
                };
                notes_label.set_text(&notes_text);
                open_profile_button.set_sensitive(true);
                apply_profile_button.set_sensitive(file_profile_is_installed(&profile));
                run_command_button.set_sensitive(true);
                remove_profile_button.set_sensitive(true);
                run_command_button.set_label(if file_profile_is_installed(&profile) {
                    "Update profile"
                } else {
                    "Install profile"
                });
            })
        };

        let refresh_holder: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let refresh_holder_for_ui = refresh_holder.clone();
        let refresh_ui: Rc<dyn Fn()> = {
            let store = store.clone();
            let selected_name = selected_name.clone();
            let filter_mode = filter_mode.clone();
            let sort_mode = sort_mode.clone();
            let search_entry = search_entry.clone();
            let gallery_grid = gallery_grid.clone();
            let update_detail = update_detail.clone();
            Rc::new(move || {
                *store.borrow_mut() = load_file_profile_store();
                while let Some(child) = gallery_grid.first_child() {
                    gallery_grid.remove(&child);
                }

                let query = search_entry.text().trim().to_lowercase();
                let filter = filter_mode.borrow().clone();
                let mut profiles = store.borrow().profiles.clone();
                profiles.retain(|profile| {
                    let matches_query = query.is_empty()
                        || profile.name.to_lowercase().contains(&query)
                        || profile.repo_url.to_lowercase().contains(&query);
                    let matches_filter = match filter.as_str() {
                        "installed" => file_profile_is_installed(profile),
                        "missing" => !file_profile_is_installed(profile),
                        _ => true,
                    };
                    matches_query && matches_filter
                });

                match sort_mode.borrow().as_str() {
                    "name" => profiles.sort_by_key(|profile| profile.name.to_lowercase()),
                    "repo" => profiles.sort_by_key(|profile| profile.repo_url.to_lowercase()),
                    _ => profiles.sort_by_key(|profile| {
                        if Some(profile.name.clone()) == *selected_name.borrow() {
                            0
                        } else {
                            1
                        }
                    }),
                }

                let selected = selected_name.borrow().clone();
                let mut selected_profile = None;
                for (index, profile) in profiles.iter().enumerate() {
                    let card = Button::new();
                    card.add_css_class("wallpaper-card");
                    card.set_hexpand(true);
                    card.set_vexpand(false);
                    card.set_size_request(220, 150);
                    card.set_tooltip_text(Some("Select this profile to inspect and install it"));
                    if selected.as_deref() == Some(profile.name.as_str()) {
                        card.add_css_class("wallpaper-card-selected");
                        selected_profile = Some(profile.clone());
                    }

                    let card_box = Box::new(Orientation::Vertical, 5);
                    card_box.set_margin_top(8);
                    card_box.set_margin_bottom(8);
                    card_box.set_margin_start(8);
                    card_box.set_margin_end(8);
                    let preview = Label::new(Some(&file_profile_preview(profile)));
                    preview.set_halign(gtk::Align::Start);
                    preview.set_valign(gtk::Align::Start);
                    preview.set_xalign(0.0);
                    preview.set_yalign(0.0);
                    preview.set_wrap(true);
                    preview.set_max_width_chars(30);
                    preview.set_size_request(-1, 82);
                    let name = Label::new(Some(&profile.name));
                    name.set_markup(&format!(
                        "<b>{}</b>",
                        glib::markup_escape_text(&profile.name)
                    ));
                    name.set_halign(gtk::Align::Start);
                    let status = Label::new(Some(file_profile_status(profile)));
                    status.set_halign(gtk::Align::Start);
                    status.set_opacity(0.75);
                    card_box.append(&preview);
                    card_box.append(&name);
                    card_box.append(&status);
                    card.set_child(Some(&card_box));

                    let profile_for_card = profile.clone();
                    let store_for_card = store.clone();
                    let selected_for_card = selected_name.clone();
                    let update_detail_for_card = update_detail.clone();
                    let refresh_holder_for_card = refresh_holder_for_ui.clone();
                    card.connect_clicked(move |_| {
                        {
                            let mut store = store_for_card.borrow_mut();
                            store.selected = Some(profile_for_card.name.clone());
                            save_file_profile_store(&store);
                        }
                        *selected_for_card.borrow_mut() = Some(profile_for_card.name.clone());
                        update_detail_for_card(Some(profile_for_card.clone()));
                        if let Some(refresh) = refresh_holder_for_card.borrow().clone() {
                            refresh();
                        }
                    });

                    gallery_grid.attach(&card, (index % 3) as i32, (index / 3) as i32, 1, 1);
                }

                if selected_profile.is_none() {
                    selected_profile = profiles.first().cloned();
                    if let Some(profile) = selected_profile.clone() {
                        *selected_name.borrow_mut() = Some(profile.name.clone());
                        let mut store = store.borrow_mut();
                        store.selected = Some(profile.name.clone());
                        save_file_profile_store(&store);
                    }
                }

                update_detail(selected_profile);
            })
        };

        *refresh_holder.borrow_mut() = Some(refresh_ui.clone());
        *self.file_profiles_refresh.borrow_mut() = Some(refresh_ui.clone());

        let refresh_for_search = refresh_ui.clone();
        search_entry.connect_changed(move |_| refresh_for_search());
        let refresh_for_clear = refresh_ui.clone();
        let search_for_clear = search_entry.clone();
        clear_search_button.connect_clicked(move |_| {
            search_for_clear.set_text("");
            refresh_for_clear();
        });

        let refresh_for_sort = refresh_ui.clone();
        let sort_mode_for_dropdown = sort_mode.clone();
        sort_dropdown.connect_selected_notify(move |dropdown| {
            let value = dropdown
                .selected_item()
                .and_then(|item| item.downcast::<gtk::StringObject>().ok())
                .map(|item| item.string().to_string())
                .unwrap_or_default();
            *sort_mode_for_dropdown.borrow_mut() = match value.as_str() {
                "Name" => "name".to_string(),
                "Repository" => "repo".to_string(),
                _ => "recent".to_string(),
            };
            refresh_for_sort();
        });

        for (button, mode) in [
            (&all_filter_button, "all"),
            (&installed_filter_button, "installed"),
            (&missing_filter_button, "missing"),
        ] {
            let filter_mode = filter_mode.clone();
            let refresh = refresh_ui.clone();
            button.connect_clicked(move |_| {
                *filter_mode.borrow_mut() = mode.to_string();
                refresh();
            });
        }

        let parent = self.window.clone();
        let store_for_add = self.file_profiles.clone();
        let refresh_for_add = refresh_ui.clone();
        let selected_for_add = selected_name.clone();
        add_profile_button.connect_clicked(move |_| {
            let parent = parent.clone();
            let store_for_add = store_for_add.clone();
            let refresh_for_add = refresh_for_add.clone();
            let selected_for_add = selected_for_add.clone();
            glib::MainContext::default().spawn_local(async move {
                let dialog = gtk::Dialog::with_buttons(
                    Some("Add .file profile"),
                    Some(&parent),
                    gtk::DialogFlags::MODAL,
                    &[
                        ("Cancel", gtk::ResponseType::Cancel),
                        ("Add", gtk::ResponseType::Accept),
                    ],
                );
                let content = dialog.content_area();
                content.set_spacing(8);
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
                notes_entry.set_placeholder_text(Some("Optional notes"));
                for (label, entry) in [
                    ("Name", name_entry.clone()),
                    ("Repository URL", repo_entry.clone()),
                    ("Install path", path_entry.clone()),
                    ("Branch / ref", version_entry.clone()),
                    ("Notes", notes_entry.clone()),
                ] {
                    content.append(&Label::new(Some(label)));
                    content.append(&entry);
                }

                if dialog.run_future().await == gtk::ResponseType::Accept {
                    let profile = FileProfile {
                        name: name_entry.text().trim().to_string(),
                        repo_url: repo_entry.text().trim().to_string(),
                        install_path: path_entry.text().trim().to_string(),
                        version_ref: version_entry.text().trim().to_string(),
                        notes: notes_entry.text().trim().to_string(),
                    };
                    if profile.name.is_empty() || profile.repo_url.is_empty() {
                        show_message_dialog(
                            &parent,
                            gtk::MessageType::Warning,
                            "Missing data",
                            "A profile name and repository URL are required.",
                        );
                    } else if let Err(error) = validate_repository_url(&profile.repo_url) {
                        show_message_dialog(
                            &parent,
                            gtk::MessageType::Warning,
                            "Invalid Repository",
                            &error,
                        );
                    } else {
                        *selected_for_add.borrow_mut() = Some(profile.name.clone());
                        {
                            let mut store = store_for_add.borrow_mut();
                            store.profiles.retain(|item| item.name != profile.name);
                            store.selected = Some(profile.name.clone());
                            store.profiles.push(profile);
                            save_file_profile_store(&store);
                        }
                        refresh_for_add();
                    }
                }
                dialog.close();
            });
        });

        let parent = self.window.clone();
        let store_for_open = self.file_profiles.clone();
        let selected_for_open = selected_name.clone();
        open_profile_button.connect_clicked(move |_| {
            let selected = selected_for_open.borrow().clone();
            let store = store_for_open.borrow();
            if let Some(profile) = selected.and_then(|name| {
                store
                    .profiles
                    .iter()
                    .find(|item| item.name == name)
                    .cloned()
            }) {
                open_uri(&parent, &profile.repo_url);
            }
        });

        let parent = self.window.clone();
        let store_for_apply = self.file_profiles.clone();
        let selected_for_apply = selected_name.clone();
        let refresh_for_apply = refresh_ui.clone();
        let success_parent_for_apply = parent.clone();
        apply_profile_button.connect_clicked(move |button| {
            let Some(selected) = selected_for_apply.borrow().clone() else {
                show_message_dialog(
                    &parent,
                    gtk::MessageType::Warning,
                    "Nothing selected",
                    "Select a profile first.",
                );
                return;
            };
            let Some(profile) = store_for_apply
                .borrow()
                .profiles
                .iter()
                .find(|item| item.name == selected)
                .cloned()
            else {
                return;
            };
            let refresh_after_apply = refresh_for_apply.clone();
            let success_parent = success_parent_for_apply.clone();
            run_background_task_with_completion(
                &parent,
                Some(button),
                "Applying profile…",
                "Profile Applied",
                "The selected profile is now active.",
                "Profile Apply Failed",
                Some(std::boxed::Box::new(move || {
                    refresh_after_apply();
                    show_install_result(
                        &success_parent,
                        "Profile Applied",
                        true,
                        "The selected profile is now active.",
                    );
                })),
                move || apply_file_profile(&profile).map(|_| ()),
            );
        });

        let parent = self.window.clone();
        let store_for_run = self.file_profiles.clone();
        let selected_for_run = selected_name.clone();
        let refresh_for_run = refresh_ui.clone();
        let success_parent_for_run = parent.clone();
        run_command_button.connect_clicked(move |button| {
            let selected = selected_for_run.borrow().clone();
            let store = store_for_run.borrow();
            if let Some(profile) = selected.and_then(|name| {
                store
                    .profiles
                    .iter()
                    .find(|item| item.name == name)
                    .cloned()
            }) {
                let refresh_after_install = refresh_for_run.clone();
                let success_parent = success_parent_for_run.clone();
                install_file_profile(
                    &parent,
                    button,
                    &profile,
                    Some(std::boxed::Box::new(move || {
                        refresh_after_install();
                        show_install_result(
                            &success_parent,
                            "Dotfiles Ready",
                            true,
                            "The selected dotfiles profile is installed or updated.",
                        );
                    })),
                );
            }
        });

        let parent = self.window.clone();
        let store_for_remove = self.file_profiles.clone();
        let selected_for_remove = selected_name.clone();
        let refresh_for_remove = refresh_ui.clone();
        remove_profile_button.connect_clicked(move |_| {
            let Some(selected) = selected_for_remove.borrow().clone() else {
                show_message_dialog(
                    &parent,
                    gtk::MessageType::Warning,
                    "Nothing selected",
                    "Select a profile first.",
                );
                return;
            };
            {
                let mut store = store_for_remove.borrow_mut();
                store.profiles.retain(|profile| profile.name != selected);
                store.selected = store.profiles.first().map(|profile| profile.name.clone());
                *selected_for_remove.borrow_mut() = store.selected.clone();
                save_file_profile_store(&store);
            }
            refresh_for_remove();
        });

        let parent = self.window.clone();
        let store_for_quick = self.file_profiles.clone();
        let selected_for_quick = selected_name.clone();
        let refresh_for_quick = refresh_ui.clone();
        quick_install_button.connect_clicked(move |button| {
            let repo_url = quick_repo_entry.text().trim().to_string();
            if repo_url.is_empty() {
                show_message_dialog(
                    &parent,
                    gtk::MessageType::Warning,
                    "Missing repository",
                    "Paste a repository URL before installing.",
                );
                return;
            }
            if let Err(error) = validate_repository_url(&repo_url) {
                show_message_dialog(
                    &parent,
                    gtk::MessageType::Warning,
                    "Invalid Repository",
                    &error,
                );
                return;
            }
            let profile = FileProfile {
                name: file_profile_name_from_url(&repo_url),
                repo_url,
                install_path: quick_path_entry.text().trim().to_string(),
                version_ref: quick_ref_entry.text().trim().to_string(),
                notes: "Added from the quick install form.".to_string(),
            };
            {
                let mut store = store_for_quick.borrow_mut();
                store.profiles.retain(|item| {
                    normalize_git_remote_identity(&item.repo_url)
                        != normalize_git_remote_identity(&profile.repo_url)
                });
                store.selected = Some(profile.name.clone());
                store.profiles.push(profile.clone());
                save_file_profile_store(&store);
            }
            *selected_for_quick.borrow_mut() = Some(profile.name.clone());
            refresh_for_quick();
            let refresh_after_install = refresh_for_quick.clone();
            let success_parent = parent.clone();
            install_file_profile(
                &parent,
                button,
                &profile,
                Some(std::boxed::Box::new(move || {
                    refresh_after_install();
                    show_install_result(
                        &success_parent,
                        "Dotfiles Ready",
                        true,
                        "The selected dotfiles profile is installed or updated.",
                    );
                })),
            );
        });

        let refresh_for_button = refresh_ui.clone();
        refresh_button.connect_clicked(move |_| refresh_for_button());

        container.append(&header_row);
        container.append(&search_row);
        container.append(&filter_row);
        container.append(&body_row);
        container.append(&quick_install_frame);
        scrolled_window.set_child(Some(&container));
        self.stack
            .add_titled(&scrolled_window, Some("files"), ".files");
        refresh_ui();
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
        description_label.set_hexpand(true);
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
            "Examples: `main`, `v0.1.0`, `dc92648`, or `github:NixOS/nixpkgs/<ref>` on NixOS. Hard Update replaces the local GUI checkout from GitHub.",
        ));
        version_help_label.set_wrap(true);
        version_help_label.set_halign(gtk::Align::Start);
        version_help_label.set_opacity(0.72);

        let install_hyprland_button = Button::with_label("Install Hyprland");
        let update_hyprland_button = Button::with_label("Update Hyprland");
        let hard_update_button = Button::with_label("Hard Update");
        let hard_delete_button = Button::with_label("Hard Delete");

        let parent = self.window.clone();
        let hyprland_version_for_install = hyprland_version_entry.clone();
        install_hyprland_button.connect_clicked(move |button| {
            let version_ref = entry_text_or_none(&hyprland_version_for_install);
            install_hyprland_from_gui(&parent, button, version_ref.as_deref());
        });

        let parent = self.window.clone();
        let hyprland_version_for_update = hyprland_version_entry.clone();
        update_hyprland_button.connect_clicked(move |button| {
            let version_ref = entry_text_or_none(&hyprland_version_for_update);
            update_hyprland_from_gui(&parent, button, version_ref.as_deref());
        });

        let parent = self.window.clone();
        let software_version_for_update = software_version_entry.clone();
        hard_update_button.connect_clicked(move |button| {
            let version_ref = entry_text_or_none(&software_version_for_update);
            confirm_hard_update(&parent, button, version_ref);
        });

        let parent = self.window.clone();
        hard_delete_button.connect_clicked(move |button| {
            confirm_hard_delete(&parent, button);
        });

        let button_row = Box::new(Orientation::Vertical, 10);
        button_row.set_hexpand(true);
        install_hyprland_button.set_hexpand(true);
        update_hyprland_button.set_hexpand(true);
        hard_update_button.set_hexpand(true);
        hard_delete_button.set_hexpand(true);
        button_row.append(&install_hyprland_button);
        button_row.append(&update_hyprland_button);
        button_row.append(&hard_update_button);
        button_row.append(&hard_delete_button);

        let checklist_label = Label::new(Some(
            "Recommended path: use Hard Update to replace and rebuild the GUI checkout. Hard Delete removes only the GUI installation; it does not remove Hyprland or dotfiles.",
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
        self.stack.add_titled(
            &scrolled_window,
            Some("hyprland-install"),
            "Hyprland Install",
        );
    }

    pub fn load_landing_pages(&mut self, note: &str) {
        self.config_widgets.clear();
        self.changed_options.borrow_mut().clear();

        self.rebuild_navigation();
        self.add_setup_overview_page(note);
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
            .transient_for(&self.window)
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
            .transient_for(&self.window)
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
            .transient_for(&self.window)
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
        desc_label.set_wrap(true);
        desc_label.set_hexpand(true);

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
        label_widget.set_wrap(true);
        label_widget.set_hexpand(true);
        label_widget.set_xalign(0.0);

        let tooltip_button = Button::new();
        let question_mark_icon = icon_image("dialog-question-symbolic");
        tooltip_button.set_child(Some(&question_mark_icon));
        tooltip_button.set_has_frame(false);

        let popover = Popover::new();
        let description_label = Label::new(Some(description));
        description_label.set_margin_top(5);
        description_label.set_margin_bottom(5);
        description_label.set_margin_start(5);
        description_label.set_margin_end(5);
        description_label.set_wrap(true);
        description_label.set_max_width_chars(56);
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
        label_widget.set_wrap(true);
        label_widget.set_hexpand(true);
        label_widget.set_xalign(0.0);

        let tooltip_button = Button::new();
        let question_mark_icon = icon_image("dialog-question-symbolic");
        tooltip_button.set_child(Some(&question_mark_icon));
        tooltip_button.set_has_frame(false);

        let popover = Popover::new();
        let description_label = Label::new(Some(description));
        description_label.set_margin_top(5);
        description_label.set_margin_bottom(5);
        description_label.set_margin_start(5);
        description_label.set_margin_end(5);
        description_label.set_wrap(true);
        description_label.set_max_width_chars(56);
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
        label_widget.set_wrap(true);
        label_widget.set_hexpand(true);
        label_widget.set_xalign(0.0);

        let tooltip_button = Button::new();
        let question_mark_icon = icon_image("dialog-question-symbolic");
        tooltip_button.set_child(Some(&question_mark_icon));
        tooltip_button.set_has_frame(false);

        let popover = Popover::new();
        let description_label = Label::new(Some(description));
        description_label.set_margin_top(5);
        description_label.set_margin_bottom(5);
        description_label.set_margin_start(5);
        description_label.set_margin_end(5);
        description_label.set_wrap(true);
        description_label.set_max_width_chars(56);
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
        label_widget.set_wrap(true);
        label_widget.set_hexpand(true);
        label_widget.set_xalign(0.0);

        let tooltip_button = Button::new();
        let question_mark_icon = icon_image("dialog-question-symbolic");
        tooltip_button.set_child(Some(&question_mark_icon));
        tooltip_button.set_has_frame(false);

        let popover = Popover::new();
        let description_label = Label::new(Some(description));
        description_label.set_margin_top(5);
        description_label.set_margin_bottom(5);
        description_label.set_margin_start(5);
        description_label.set_margin_end(5);
        description_label.set_wrap(true);
        description_label.set_max_width_chars(56);
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
        label_widget.set_wrap(true);
        label_widget.set_hexpand(true);
        label_widget.set_xalign(0.0);

        let tooltip_button = Button::new();
        let question_mark_icon = icon_image("dialog-question-symbolic");
        tooltip_button.set_child(Some(&question_mark_icon));
        tooltip_button.set_has_frame(false);

        let popover = Popover::new();
        let description_label = Label::new(Some(description));
        description_label.set_margin_top(5);
        description_label.set_margin_bottom(5);
        description_label.set_margin_start(5);
        description_label.set_margin_end(5);
        description_label.set_wrap(true);
        description_label.set_max_width_chars(56);
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

#[cfg(test)]
mod tests {
    use super::{
        collect_profile_files, normalize_git_remote_identity, profile_copy_plan, safe_home_path,
        validate_repository_url, validate_version_ref,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "better-hyprland-gui-dotfiles-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    #[test]
    fn normalizes_common_github_remote_forms() {
        let expected = "github.com/example/dotfiles";

        assert_eq!(
            normalize_git_remote_identity("https://github.com/example/dotfiles.git"),
            expected
        );
        assert_eq!(
            normalize_git_remote_identity("git@github.com:example/dotfiles.git"),
            expected
        );
        assert_eq!(
            normalize_git_remote_identity("[repo](https://github.com/example/dotfiles)"),
            expected
        );
        assert_eq!(
            normalize_git_remote_identity(
                "https://www.youtube.com/redirect?event=video_description&q=https://github.com/example/dotfiles&v=1"
            ),
            expected
        );
    }

    #[test]
    fn rejects_refs_that_can_be_parsed_as_options() {
        assert!(validate_version_ref("--detach").is_err());
        assert!(validate_version_ref("feature branch").is_err());
        assert!(validate_version_ref("main").is_ok());
        assert!(validate_version_ref("v0.1.4").is_ok());
    }

    #[test]
    fn validates_clone_urls_and_rejects_redirect_links() {
        assert!(validate_repository_url("https://github.com/example/dotfiles.git").is_ok());
        assert!(validate_repository_url("git@github.com:example/dotfiles.git").is_ok());
        assert!(validate_repository_url(
            "https://www.youtube.com/redirect?event=video_description&q=https://github.com/example/dotfiles"
        )
        .is_err());
        assert!(validate_repository_url("https://github.com/example").is_err());
    }

    #[test]
    fn builds_plan_for_home_and_hypr_layouts() {
        let root = test_root("home-layout");
        let profile = root.join("profile");
        let home = root.join("home");
        fs::create_dir_all(profile.join("dots/.config/hypr")).expect("create profile layout");
        fs::write(profile.join("dots/.config/hypr/hyprland.conf"), "general {}")
            .expect("write hyprland config");

        let plan = profile_copy_plan(&profile, &home).expect("build copy plan");
        assert!(plan.iter().any(|(source, destination)| {
            source.ends_with(Path::new("dots/.config"))
                && destination.ends_with(Path::new(".config"))
        }));
        assert!(plan.iter().any(|(source, destination)| {
            source.ends_with(Path::new("dots/.config/hypr"))
                && destination.ends_with(Path::new(".config/hypr"))
        }));
    }

    #[test]
    fn builds_plan_for_stow_style_packages() {
        let root = test_root("stow-layout");
        let profile = root.join("profile");
        let home = root.join("home");
        fs::create_dir_all(profile.join("hyprland/.config/hypr")).expect("create stow layout");
        fs::write(profile.join("hyprland/.config/hypr/hyprland.conf"), "general {}")
            .expect("write hyprland config");

        let plan = profile_copy_plan(&profile, &home).expect("build stow copy plan");
        assert!(plan.iter().any(|(source, destination)| {
            source.ends_with(Path::new("hyprland/.config"))
                && destination.ends_with(Path::new(".config"))
        }));
    }

    #[test]
    fn rejects_unsupported_profiles_and_unsafe_managed_paths() {
        let root = test_root("invalid-layout");
        let profile = root.join("profile");
        let home = root.join("home");
        fs::create_dir_all(&profile).expect("create empty profile");

        assert!(profile_copy_plan(&profile, &home).is_err());
        assert!(safe_home_path(&home, "../outside").is_err());
        assert!(safe_home_path(&home, "/tmp/outside").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn allows_internal_profile_symlinks_but_rejects_external_links() {
        use std::os::unix::fs::symlink;

        let root = test_root("symlink-safety");
        let profile = root.join("profile");
        let home = root.join("home");
        let icons = profile.join("dots/.config/icons");
        fs::create_dir_all(&icons).expect("create profile icons");
        fs::write(icons.join("base.svg"), "<svg />").expect("write target icon");
        symlink("base.svg", icons.join("alias.svg")).expect("create internal symlink");

        let canonical_profile = fs::canonicalize(&profile).expect("resolve profile root");
        let mut manifest = Vec::new();
        collect_profile_files(
            &profile.join("dots/.config/icons"),
            &home.join(".config/icons"),
            &home,
            &canonical_profile,
            &mut manifest,
        )
        .expect("allow internal symlink");
        assert!(manifest.iter().any(|entry| entry == ".config/icons/alias.svg"));

        let outside = root.join("outside.svg");
        fs::write(&outside, "<svg />").expect("write outside target");
        symlink(&outside, icons.join("outside.svg")).expect("create external symlink");
        let error = collect_profile_files(
            &profile.join("dots/.config/icons"),
            &home.join(".config/icons"),
            &home,
            &canonical_profile,
            &mut Vec::new(),
        )
        .expect_err("reject external symlink");
        assert!(error.contains("outside the installed profile"));
    }
}
