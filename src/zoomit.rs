//! ZoomIt's zoom and screen recording. (Drawing lives in draw.rs.)

use crate::util;
use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const MAX_ZOOM: f64 = 8.0;
const STEP: f64 = 1.5;

#[derive(Clone, Copy, ValueEnum)]
pub enum ZoomAction {
    In,
    Out,
    Reset,
    /// 2× if not zoomed, back to normal if zoomed
    Toggle,
}

/// Hyprland's current zoom (1.0 = normal).
pub fn zoom_factor() -> f64 {
    // `hyprctl getoption` prints e.g. "float: 1.500000".
    util::output("hyprctl", &["getoption", "cursor:zoom_factor"])
        .ok()
        .and_then(|out| out.lines().next().and_then(|l| l.split_whitespace().last()).and_then(|v| v.parse().ok()))
        .unwrap_or(1.0)
}

fn next_zoom(current: f64, action: ZoomAction) -> f64 {
    let z = match action {
        ZoomAction::In => current * STEP,
        ZoomAction::Out => current / STEP,
        ZoomAction::Reset => 1.0,
        ZoomAction::Toggle if current > 1.01 => 1.0,
        ZoomAction::Toggle => 2.0,
    };
    // Snap tiny leftovers back to exactly 1×.
    let z = z.clamp(1.0, MAX_ZOOM);
    if (z - 1.0).abs() < 0.05 { 1.0 } else { z }
}

/// Zooms the screen around the mouse using Hyprland's built-in zoom.
pub fn zoom(action: ZoomAction) -> Result<f64> {
    let z = next_zoom(zoom_factor(), action);
    let reply = util::output("hyprctl", &["eval", &format!("hl.config({{ cursor = {{ zoom_factor = {z} }} }})")])?;
    if reply.starts_with("error") {
        bail!("Hyprland rejected the zoom: {reply}");
    }
    Ok(z)
}

fn state_dir() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(runtime).join("quiver")
}

/// The recording in progress, if any: (pid, output file).
pub fn recording() -> Option<(u32, PathBuf)> {
    let pid: u32 = std::fs::read_to_string(state_dir().join("record.pid")).ok()?.trim().parse().ok()?;
    // Stale pid file? `kill -0` only checks the process exists.
    let alive = Command::new("kill").args(["-0", &pid.to_string()]).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    let file = std::fs::read_to_string(state_dir().join("record.file")).ok()?;
    alive.then(|| (pid, PathBuf::from(file.trim())))
}

fn videos_dir() -> PathBuf {
    util::output("xdg-user-dir", &["VIDEOS"])
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Videos"))
}

/// slurp's "x,y wxh" → gpu-screen-recorder's "WxH+X+Y".
fn gsr_region(slurp: &str) -> Option<String> {
    let (pos, size) = slurp.trim().split_once(' ')?;
    let (x, y) = pos.split_once(',')?;
    let (w, h) = size.split_once('x')?;
    Some(format!("{w}x{h}+{x}+{y}"))
}

/// Starts recording the focused monitor (or a dragged region), or stops the
/// recording in progress. Returns a message for the user.
pub fn record(region: bool) -> Result<String> {
    if let Some((pid, file)) = recording() {
        // SIGINT makes gpu-screen-recorder finish the file properly.
        Command::new("kill").args(["-INT", &pid.to_string()]).status()?;
        let _ = std::fs::remove_file(state_dir().join("record.pid"));
        let msg = format!("Saved {}", file.display());
        util::notify("Recording saved", &file.display().to_string());
        return Ok(msg);
    }

    let mut args: Vec<String> = Vec::new();
    if region {
        let Ok(area) = util::output("slurp", &[]) else { return Ok("Cancelled".into()) };
        args.extend(["-w".into(), "region".into(), "-region".into(), gsr_region(&area).context("odd region from slurp")?]);
    } else {
        let monitors = util::hypr_query("monitors")?;
        let name = monitors
            .as_array()
            .and_then(|m| m.iter().find(|m| m["focused"].as_bool() == Some(true)))
            .and_then(|m| m["name"].as_str())
            .context("no focused monitor")?;
        args.extend(["-w".into(), name.to_string()]);
    }
    let dir = videos_dir();
    std::fs::create_dir_all(&dir)?;
    let stamp = util::output("date", &["+%Y-%m-%d_%H-%M-%S"]).unwrap_or_else(|_| "recording".into());
    let file = dir.join(format!("quiver_{stamp}.mp4"));
    args.extend(["-f".into(), "60".into(), "-o".into(), file.to_string_lossy().into_owned()]);

    let child = Command::new("gpu-screen-recorder")
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to start `gpu-screen-recorder` (is it installed?)")?;
    std::fs::create_dir_all(state_dir())?;
    std::fs::write(state_dir().join("record.pid"), child.id().to_string())?;
    std::fs::write(state_dir().join("record.file"), file.to_string_lossy().as_bytes())?;
    util::notify("Recording", "Run the same shortcut again to stop");
    Ok(format!("Recording to {}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_steps_and_limits() {
        assert_eq!(next_zoom(1.0, ZoomAction::In), 1.5);
        assert_eq!(next_zoom(1.5, ZoomAction::Out), 1.0);
        assert_eq!(next_zoom(1.0, ZoomAction::Out), 1.0);
        assert_eq!(next_zoom(7.0, ZoomAction::In), MAX_ZOOM);
        assert_eq!(next_zoom(1.0, ZoomAction::Toggle), 2.0);
        assert_eq!(next_zoom(3.0, ZoomAction::Toggle), 1.0);
    }

    #[test]
    fn converts_slurp_region() {
        assert_eq!(gsr_region("10,20 300x200").as_deref(), Some("300x200+10+20"));
        assert_eq!(gsr_region("garbage"), None);
    }
}
