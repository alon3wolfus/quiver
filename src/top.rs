use crate::util;
use anyhow::{Context, Result};

/// Toggles "always on top" for the focused window. Hyprland's pin keeps a floating
/// window above tiled ones and shows it on every workspace.
pub fn toggle() -> Result<()> {
    let active = util::hypr_query("activewindow")?;
    let address = active["address"].as_str().context("no focused window")?;
    let title = active["title"].as_str().unwrap_or_default();

    // Hyprland silently ignores pin on tiled windows, and floating one would pull it
    // out of the tiling layout, so leave tiled windows alone.
    if active["floating"].as_bool() != Some(true) {
        util::notify("Can't pin a tiled window", "Only floating windows can stay on top. Float it first.");
        return Ok(());
    }

    util::dispatch(&[format!("hl.dsp.window.pin({{ window = \"address:{address}\" }})")])?;
    let pinned = active["pinned"].as_bool() == Some(true);
    util::notify(if pinned { "Unpinned" } else { "Always on top" }, title);
    Ok(())
}
