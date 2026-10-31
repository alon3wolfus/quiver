//! Monitor settings: read them from Hyprland, apply changes live, and save them
//! as a Lua file the Hyprland config can load.

use crate::util;
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::PathBuf;

/// One monitor as Hyprland reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct Monitor {
    pub name: String,
    pub description: String,
    pub settings: Settings,
    /// e.g. "1920x1080@60.00"
    pub modes: Vec<String>,
}

/// The parts of a monitor's setup quiver lets you change.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub output: String,
    /// "WIDTHxHEIGHT@REFRESH", e.g. "1920x1080@60.00"
    pub mode: String,
    pub x: i64,
    pub y: i64,
    pub scale: f64,
    /// 0 normal, 1 = 90°, 2 = 180°, 3 = 270°
    pub transform: i64,
    pub disabled: bool,
}

fn parse(m: &Value) -> Option<Monitor> {
    let name = m["name"].as_str()?.to_string();
    let modes: Vec<String> = m["availableModes"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(|s| s.trim_end_matches("Hz").to_string()).collect())
        .unwrap_or_default();
    let mode = format!(
        "{}x{}@{:.2}",
        m["width"].as_i64()?,
        m["height"].as_i64()?,
        m["refreshRate"].as_f64().unwrap_or(60.0)
    );
    Some(Monitor {
        description: m["description"].as_str().unwrap_or_default().to_string(),
        settings: Settings {
            output: name.clone(),
            mode,
            x: m["x"].as_i64().unwrap_or(0),
            y: m["y"].as_i64().unwrap_or(0),
            scale: m["scale"].as_f64().unwrap_or(1.0),
            transform: m["transform"].as_i64().unwrap_or(0),
            disabled: m["disabled"].as_bool().unwrap_or(false),
        },
        name,
        modes,
    })
}

/// Every connected monitor, including disabled ones.
pub fn monitors() -> Result<Vec<Monitor>> {
    let list = util::hypr_query("monitors all")?;
    Ok(list.as_array().context("unexpected hyprctl output")?.iter().filter_map(parse).collect())
}

/// The Hyprland Lua call for these settings.
pub fn lua(s: &Settings) -> String {
    let output = util::lua_str(&s.output);
    if s.disabled {
        return format!("hl.monitor({{ output = {output}, disabled = true }})");
    }
    format!(
        "hl.monitor({{ output = {output}, mode = {}, position = \"{}x{}\", scale = {}, transform = {} }})",
        util::lua_str(&s.mode),
        s.x,
        s.y,
        s.scale,
        s.transform
    )
}

/// Applies settings right away. They last until Hyprland restarts unless saved.
pub fn apply(settings: &[Settings]) -> Result<()> {
    if settings.iter().all(|s| s.disabled) {
        bail!("at least one monitor has to stay on");
    }
    let code: Vec<String> = settings.iter().map(lua).collect();
    let reply = util::output("hyprctl", &["eval", &code.join("\n")])?;
    if reply.starts_with("error") {
        bail!("Hyprland rejected the monitor settings: {reply}");
    }
    Ok(())
}

pub fn saved_path() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    config.join("quiver/monitors.lua")
}

/// The line to add to the Hyprland config so saved settings load at login.
pub fn include_line() -> String {
    "dofile(os.getenv(\"HOME\") .. \"/.config/quiver/monitors.lua\")".into()
}

/// Writes the settings to ~/.config/quiver/monitors.lua.
pub fn save(settings: &[Settings]) -> Result<PathBuf> {
    let path = saved_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    let mut text = String::from("-- Written by quiver's Displays page. Load it from your Hyprland config with:\n");
    text += &format!("-- {}\n", include_line());
    for s in settings {
        text += &lua(s);
        text.push('\n');
    }
    std::fs::write(&path, text)?;
    Ok(path)
}

/// GTK's text size multiplier (1.0 = normal), used by GTK apps everywhere.
pub fn text_scale() -> f64 {
    util::output("gsettings", &["get", "org.gnome.desktop.interface", "text-scaling-factor"])
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0)
}

pub fn set_text_scale(factor: f64) -> Result<()> {
    util::output("gsettings", &["set", "org.gnome.desktop.interface", "text-scaling-factor", &format!("{factor:.2}")])
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "name": "eDP-1", "description": "BOE 0x0BCA", "width": 1920, "height": 1080,
            "refreshRate": 60.003, "x": 0, "y": 0, "scale": 1.25, "transform": 0, "disabled": false,
            "availableModes": ["1920x1080@60.00Hz", "1920x1080@48.00Hz"]
        })
    }

    #[test]
    fn reads_hyprland_monitor_json() {
        let m = parse(&sample()).unwrap();
        assert_eq!(m.settings.mode, "1920x1080@60.00");
        assert_eq!(m.modes, ["1920x1080@60.00", "1920x1080@48.00"]);
        assert_eq!(m.settings.scale, 1.25);
    }

    #[test]
    fn builds_lua_calls() {
        let mut s = parse(&sample()).unwrap().settings;
        s.x = 1920;
        s.transform = 1;
        assert_eq!(
            lua(&s),
            r#"hl.monitor({ output = "eDP-1", mode = "1920x1080@60.00", position = "1920x0", scale = 1.25, transform = 1 })"#
        );
        s.disabled = true;
        assert_eq!(lua(&s), r#"hl.monitor({ output = "eDP-1", disabled = true })"#);
    }

    #[test]
    fn refuses_to_turn_every_monitor_off() {
        let mut s = parse(&sample()).unwrap().settings;
        s.disabled = true;
        assert!(apply(&[s]).is_err());
    }
}
