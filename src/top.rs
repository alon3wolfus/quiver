use crate::util;
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::PathBuf;

/// Hyprland can only pin floating windows. For a tiled window we float it exactly
/// where its tile was, pin it, and leave a marker so unpinning puts it back in the grid.
fn marker(address: &str) -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(runtime).join("quiver/retile").join(address)
}

/// The dispatchers to toggle pinning, given the window's hyprctl JSON and whether
/// quiver floated it when pinning.
fn plan(win: &Value, floated_by_us: bool) -> Vec<String> {
    let address = win["address"].as_str().unwrap_or_default();
    let window = format!("window = \"address:{address}\"");
    let pin = format!("hl.dsp.window.pin({{ {window} }})");

    if win["pinned"].as_bool() == Some(true) {
        let mut out = vec![pin];
        if floated_by_us {
            out.push(format!("hl.dsp.window.float({{ action = \"unset\", {window} }})"));
        }
        return out;
    }
    if win["floating"].as_bool() == Some(true) {
        return vec![pin];
    }
    // Floating would otherwise resize it and (with a centring window rule) move it,
    // so put it back on its tile's exact spot before pinning.
    let xy = |key: &str, i: usize| win[key][i].as_i64().unwrap_or(0);
    vec![
        format!("hl.dsp.window.float({{ action = \"set\", {window} }})"),
        format!("hl.dsp.window.resize({{ x = {}, y = {}, {window} }})", xy("size", 0), xy("size", 1)),
        format!("hl.dsp.window.move({{ x = {}, y = {}, {window} }})", xy("at", 0), xy("at", 1)),
        pin,
    ]
}

/// Toggles "always on top" for the focused window, or the one at `address`.
pub fn toggle(address: Option<&str>) -> Result<()> {
    let win = match address {
        Some(address) => {
            let clients = util::hypr_query("clients")?;
            let found = clients.as_array().and_then(|c| c.iter().find(|w| w["address"] == address));
            match found {
                Some(w) => w.clone(),
                None => bail!("no window with address {address}"),
            }
        }
        None => util::hypr_query("activewindow")?,
    };
    let address = win["address"].as_str().context("no focused window")?.to_string();
    let title = win["title"].as_str().unwrap_or_default();
    let marker = marker(&address);
    let pinned = win["pinned"].as_bool() == Some(true);
    let tiled = win["floating"].as_bool() != Some(true);

    util::dispatch(&plan(&win, marker.exists()))?;

    if pinned {
        let _ = std::fs::remove_file(&marker);
        util::notify("Unpinned", title);
    } else {
        if tiled {
            std::fs::create_dir_all(marker.parent().unwrap())?;
            std::fs::write(&marker, "")?;
        }
        util::notify("Always on top", title);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn win(floating: bool, pinned: bool) -> Value {
        json!({ "address": "0xabc", "floating": floating, "pinned": pinned, "at": [70, 20], "size": [900, 1040] })
    }

    #[test]
    fn tiled_window_floats_in_place_then_pins() {
        let out = plan(&win(false, false), false);
        assert_eq!(out.len(), 4);
        assert!(out[0].contains("action = \"set\""));
        assert!(out[1].contains("x = 900, y = 1040"));
        assert!(out[2].contains("x = 70, y = 20"));
        assert!(out[3].starts_with("hl.dsp.window.pin"));
    }

    #[test]
    fn unpin_retiles_only_windows_we_floated() {
        assert_eq!(plan(&win(true, true), true).len(), 2);
        assert!(plan(&win(true, true), true)[1].contains("action = \"unset\""));
        assert_eq!(plan(&win(true, true), false).len(), 1);
    }

    #[test]
    fn floating_window_just_pins() {
        assert_eq!(plan(&win(true, false), false), ["hl.dsp.window.pin({ window = \"address:0xabc\" })"]);
    }
}
