use crate::util;
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum Action {
    /// Show the available layouts
    List,
    /// Tile every window on the current workspace into a layout
    Apply {
        layout: String,
        /// Print the Hyprland dispatchers instead of running them
        #[arg(long)]
        dry_run: bool,
    },
    /// Move the focused window into one zone of a layout (zones count from 1)
    Snap {
        layout: String,
        zone: usize,
        #[arg(long)]
        dry_run: bool,
    },
    /// Write the default layouts to ~/.config/quiver/layouts.toml to edit them
    Init,
}

/// Zones are [x, y, width, height] in percent of the usable monitor area.
#[derive(Serialize, Deserialize)]
struct Config {
    #[serde(default = "default_gap")]
    gap: i64,
    layouts: BTreeMap<String, Vec<[f64; 4]>>,
}

fn default_gap() -> i64 {
    10
}

const DEFAULT_CONFIG: &str = r#"# Gap in pixels between zones and around the edges
gap = 10

# Each zone is [x, y, width, height] in percent of the screen (minus bars)
[layouts]
columns = [[0, 0, 50, 100], [50, 0, 50, 100]]
thirds = [[0, 0, 33.33, 100], [33.33, 0, 33.34, 100], [66.67, 0, 33.33, 100]]
main-stack = [[0, 0, 60, 100], [60, 0, 40, 50], [60, 50, 40, 50]]
grid = [[0, 0, 50, 50], [50, 0, 50, 50], [0, 50, 50, 50], [50, 50, 50, 50]]
focus = [[15, 5, 70, 90]]
"#;

fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("quiver/layouts.toml")
}

fn load_config() -> Result<Config> {
    let path = config_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => DEFAULT_CONFIG.to_string(),
    };
    toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))
}

/// A rectangle in Hyprland's logical (scaled) pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}

/// The focused monitor's area minus reserved space (bars), plus its active workspace id.
fn focused_monitor() -> Result<(Rect, i64)> {
    let monitors: Vec<Value> = serde_json::from_value(util::hypr_query("monitors")?)?;
    let m = monitors
        .iter()
        .find(|m| m["focused"].as_bool() == Some(true))
        .context("no focused monitor")?;
    let num = |key: &str| m[key].as_f64().unwrap_or(0.0);
    let scale = num("scale").max(0.1);
    // Odd transforms are 90/270 degree rotations, which swap width and height.
    let rotated = m["transform"].as_i64().unwrap_or(0) % 2 == 1;
    let (mut w, mut h) = (num("width") / scale, num("height") / scale);
    if rotated {
        std::mem::swap(&mut w, &mut h);
    }
    // reserved is [left, top, right, bottom]
    let r: Vec<i64> = m["reserved"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_i64().unwrap_or(0)).collect())
        .unwrap_or_else(|| vec![0; 4]);
    let area = Rect {
        x: num("x") as i64 + r[0],
        y: num("y") as i64 + r[1],
        w: w as i64 - r[0] - r[2],
        h: h as i64 - r[1] - r[3],
    };
    let workspace = m["activeWorkspace"]["id"].as_i64().context("monitor has no workspace")?;
    Ok((area, workspace))
}

/// Turns a percent zone into pixels, with `gap` around the outer edges and half a gap
/// on inner edges so neighbouring windows end up exactly `gap` apart.
fn zone_rect(area: Rect, zone: [f64; 4], gap: i64) -> Rect {
    let px = |pct: f64, size: i64| (pct / 100.0 * size as f64).round() as i64;
    let left = area.x + px(zone[0], area.w);
    let top = area.y + px(zone[1], area.h);
    let right = area.x + px(zone[0] + zone[2], area.w);
    let bottom = area.y + px(zone[1] + zone[3], area.h);
    let inset = |edge: i64, outer: i64| if edge == outer { gap } else { gap / 2 };
    let x = left + inset(left, area.x);
    let y = top + inset(top, area.y);
    let r = right - inset(right, area.x + area.w);
    let b = bottom - inset(bottom, area.y + area.h);
    Rect { x, y, w: (r - x).max(1), h: (b - y).max(1) }
}

fn place(address: &str, rect: Rect) -> Vec<String> {
    let window = format!("window = \"address:{address}\"");
    vec![
        format!("hl.dsp.window.float({{ action = \"set\", {window} }})"),
        format!("hl.dsp.window.resize({{ x = {}, y = {}, {window} }})", rect.w, rect.h),
        format!("hl.dsp.window.move({{ x = {}, y = {}, {window} }})", rect.x, rect.y),
    ]
}

fn execute(dispatchers: &[String], dry_run: bool) -> Result<()> {
    if dry_run {
        dispatchers.iter().for_each(|d| println!("hl.dispatch({d})"));
        return Ok(());
    }
    util::dispatch(dispatchers)
}

/// Layout names, for listing them in the launcher.
pub fn layout_names() -> Vec<String> {
    load_config().map(|c| c.layouts.into_keys().collect()).unwrap_or_default()
}

fn layout<'a>(config: &'a Config, name: &str) -> Result<&'a Vec<[f64; 4]>> {
    match config.layouts.get(name) {
        Some(zones) if !zones.is_empty() => Ok(zones),
        Some(_) => bail!("layout `{name}` has no zones"),
        None => bail!(
            "no layout `{name}`, available: {}",
            config.layouts.keys().cloned().collect::<Vec<_>>().join(", ")
        ),
    }
}

/// All layouts, sorted by name, for drawing previews in the settings window.
pub fn layouts() -> Result<Vec<(String, Vec<[f64; 4]>)>> {
    Ok(load_config()?.layouts.into_iter().collect())
}

/// Writes the default layouts file if there isn't one yet, and returns its path.
pub fn ensure_config() -> Result<PathBuf> {
    let path = config_path();
    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, DEFAULT_CONFIG)?;
    }
    Ok(path)
}

/// Tiles every window on the current workspace into a layout.
pub fn apply(name: &str, dry_run: bool) -> Result<()> {
    let config = load_config()?;
    let zones = layout(&config, name)?;
    let (area, workspace) = focused_monitor()?;
    let clients: Vec<Value> = serde_json::from_value(util::hypr_query("clients")?)?;
    let mut windows: Vec<&Value> = clients
        .iter()
        .filter(|c| c["workspace"]["id"].as_i64() == Some(workspace))
        .filter(|c| c["mapped"].as_bool() == Some(true) && c["hidden"].as_bool() != Some(true))
        // Leave the settings window alone when a layout is applied from it.
        .filter(|c| c["class"].as_str() != Some(crate::gui::APP_ID))
        .collect();
    // Most recently focused window gets the first (usually biggest) zone.
    windows.sort_by_key(|c| c["focusHistoryID"].as_i64().unwrap_or(i64::MAX));
    // Windows beyond the zone count wrap around and share zones.
    let commands: Vec<String> = windows
        .iter()
        .enumerate()
        .flat_map(|(i, c)| {
            let rect = zone_rect(area, zones[i % zones.len()], config.gap);
            place(c["address"].as_str().unwrap_or_default(), rect)
        })
        .collect();
    execute(&commands, dry_run)
}

pub fn run(action: Action) -> Result<()> {
    match action {
        Action::List => {
            let config = load_config()?;
            for (name, zones) in &config.layouts {
                println!("{name} ({} zones)", zones.len());
            }
        }
        Action::Init => {
            let path = config_path();
            if path.exists() {
                bail!("{} already exists", path.display());
            }
            println!("wrote {}", ensure_config()?.display());
        }
        Action::Apply { layout: name, dry_run } => apply(&name, dry_run)?,
        Action::Snap { layout: name, zone, dry_run } => {
            let config = load_config()?;
            let zones = layout(&config, &name)?;
            if zone == 0 || zone > zones.len() {
                bail!("layout `{name}` has zones 1 to {}", zones.len());
            }
            let (area, _) = focused_monitor()?;
            let active: Value = util::hypr_query("activewindow")?;
            let address = active["address"].as_str().context("no focused window")?;
            execute(&place(address, zone_rect(area, zones[zone - 1], config.gap)), dry_run)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect { x: 0, y: 0, w: 1000, h: 500 };

    #[test]
    fn halves_meet_with_one_gap_between() {
        let left = zone_rect(AREA, [0.0, 0.0, 50.0, 100.0], 10);
        let right = zone_rect(AREA, [50.0, 0.0, 50.0, 100.0], 10);
        assert_eq!(left, Rect { x: 10, y: 10, w: 485, h: 480 });
        assert_eq!(right, Rect { x: 505, y: 10, w: 485, h: 480 });
        assert_eq!(right.x - (left.x + left.w), 10);
    }

    #[test]
    fn respects_area_offset() {
        let full = zone_rect(Rect { x: 60, y: 10, ..AREA }, [0.0, 0.0, 100.0, 100.0], 0);
        assert_eq!(full, Rect { x: 60, y: 10, w: 1000, h: 500 });
    }

    #[test]
    fn default_config_parses() {
        let config: Config = toml::from_str(DEFAULT_CONFIG).unwrap();
        assert_eq!(config.layouts["grid"].len(), 4);
    }
}
