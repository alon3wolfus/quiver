use anyhow::{Context, Result, bail};
use std::io::Write;
use std::process::{Command, Stdio};

/// Runs a program and returns its trimmed stdout, failing on a non-zero exit.
pub fn output(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to start `{program}` (is it installed?)"))?;
    if !out.status.success() {
        bail!(
            "`{program}` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Puts text on the Wayland clipboard.
pub fn copy(text: &str) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to start `wl-copy` (install wl-clipboard)")?;
    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    child.wait()?;
    Ok(())
}

/// Shows a desktop notification; silently does nothing if notify-send is missing.
pub fn notify(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "quiver", "-t", "2500", title, body])
        .status();
}

/// Asks Hyprland to launch a command, so it isn't a child of quiver.
pub fn spawn(cmd: &str) -> Result<()> {
    output("hyprctl", &["dispatch", "exec", cmd]).map(|_| ())
}
