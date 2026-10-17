//! Opens a collection folder in the file manager or a terminal (R-GIT-13, D11).
//! Git commands (commit, pull, push) happen there, outside the app.

use std::path::Path;
use std::process::Command;

pub fn open_in_file_manager(path: &Path) -> Result<(), String> {
    let opened = open::that_detached(path);
    if opened.is_err() && is_wsl() {
        // WSL often has no Linux file manager: use Windows Explorer.
        if let Some(windows_path) = windows_path(path)
            && Command::new("explorer.exe")
                .arg(windows_path)
                .spawn()
                .is_ok()
        {
            return Ok(());
        }
    }
    opened.map_err(|e| format!("Could not open {}: {e}", path.display()))
}

/// Starts the platform's terminal in `path`.
pub fn open_in_terminal(path: &Path) -> Result<(), String> {
    for mut command in terminal_commands(path) {
        if command.current_dir(path).spawn().is_ok() {
            return Ok(());
        }
    }
    Err(
        "No terminal program was found. Set the TERMINAL environment variable to your terminal."
            .into(),
    )
}

/// Commands to try, best first.
fn terminal_commands(path: &Path) -> Vec<Command> {
    let mut commands = Vec::new();
    if cfg!(target_os = "windows") {
        let mut wt = Command::new("wt");
        wt.arg("-d").arg(path);
        commands.push(wt);
        let mut cmd = Command::new("cmd");
        cmd.args(["/c", "start", "cmd", "/k", "cd", "/d"]).arg(path);
        commands.push(cmd);
    } else if cfg!(target_os = "macos") {
        let mut open = Command::new("open");
        open.args(["-a", "Terminal"]).arg(path);
        commands.push(open);
    } else {
        // The user's choice first, then the desktop's default, then common terminals.
        if let Ok(terminal) = std::env::var("TERMINAL")
            && !terminal.trim().is_empty()
        {
            commands.push(Command::new(terminal.trim()));
        }
        // Under WSL: Windows Terminal (or a console window) running WSL in the folder.
        if is_wsl() {
            let mut wt = Command::new("wt.exe");
            wt.args(["wsl.exe", "--cd"]).arg(path);
            commands.push(wt);
            let mut cmd = Command::new("cmd.exe");
            cmd.args(["/c", "start", "wsl.exe", "--cd"]).arg(path);
            commands.push(cmd);
        }
        for program in [
            "x-terminal-emulator",
            "gnome-terminal",
            "konsole",
            "xfce4-terminal",
            "kitty",
            "alacritty",
            "wezterm",
            "xterm",
        ] {
            commands.push(Command::new(program));
        }
    }
    commands
}

/// Running inside the Windows Subsystem for Linux.
fn is_wsl() -> bool {
    cfg!(target_os = "linux")
        && std::fs::read_to_string("/proc/version")
            .is_ok_and(|v| v.to_ascii_lowercase().contains("microsoft"))
}

/// The Windows form of a WSL path (`\\wsl.localhost\…` or `C:\…`), via `wslpath`.
fn windows_path(path: &Path) -> Option<String> {
    let output = Command::new("wslpath").arg("-w").arg(path).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
