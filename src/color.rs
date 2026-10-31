//! Colour conversions and the recently picked colours list.

use std::path::PathBuf;

pub const FORMATS: [&str; 5] = ["hex", "rgb", "hsl", "hsv", "cmyk"];
const HISTORY_SIZE: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Parses "#697988" or "697988".
    pub fn parse_hex(text: &str) -> Option<Rgb> {
        let hex = text.trim().trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let n = u32::from_str_radix(hex, 16).ok()?;
        Some(Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
    }

    /// Channels from 0.0 to 1.0, handy for drawing with cairo.
    pub fn unit(self) -> (f64, f64, f64) {
        (self.0 as f64 / 255.0, self.1 as f64 / 255.0, self.2 as f64 / 255.0)
    }

    pub fn format(self, format: &str) -> Option<String> {
        let (r, g, b) = self.unit();
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let pct = |v: f64| (v * 100.0).round();
        Some(match format {
            "hex" => format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2),
            "rgb" => format!("rgb({}, {}, {})", self.0, self.1, self.2),
            "hsl" => {
                let l = (max + min) / 2.0;
                let s = if delta == 0.0 { 0.0 } else { delta / (1.0 - (2.0 * l - 1.0).abs()) };
                format!("hsl({}, {}%, {}%)", hue(r, g, b, max, delta), pct(s), pct(l))
            }
            "hsv" => {
                let s = if max == 0.0 { 0.0 } else { delta / max };
                format!("hsv({}, {}%, {}%)", hue(r, g, b, max, delta), pct(s), pct(max))
            }
            "cmyk" => {
                let k = 1.0 - max;
                let part = |v: f64| if k >= 1.0 { 0.0 } else { pct((1.0 - v - k) / (1.0 - k)) };
                format!("cmyk({}%, {}%, {}%, {}%)", part(r), part(g), part(b), pct(k))
            }
            _ => return None,
        })
    }

    /// Mixes towards white (amount > 0) or black (amount < 0); amount is -1.0 to 1.0.
    pub fn shade(self, amount: f64) -> Rgb {
        let target = if amount > 0.0 { 255.0 } else { 0.0 };
        let mix = |c: u8| (c as f64 + (target - c as f64) * amount.abs()).round() as u8;
        Rgb(mix(self.0), mix(self.1), mix(self.2))
    }
}

fn hue(r: f64, g: f64, b: f64, max: f64, delta: f64) -> f64 {
    if delta == 0.0 {
        return 0.0;
    }
    let h = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (h * 60.0).round()
}

fn history_path() -> PathBuf {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
        .join("quiver/colours.txt")
}

/// Recently picked colours, newest first.
pub fn history() -> Vec<Rgb> {
    std::fs::read_to_string(history_path())
        .unwrap_or_default()
        .lines()
        .filter_map(Rgb::parse_hex)
        .collect()
}

pub fn remember(colour: Rgb) -> std::io::Result<()> {
    let mut colours = history();
    colours.retain(|c| *c != colour);
    colours.insert(0, colour);
    colours.truncate(HISTORY_SIZE);
    let path = history_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    let lines: Vec<String> = colours.iter().filter_map(|c| c.format("hex")).collect();
    std::fs::write(path, lines.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_like_powertoys() {
        // The example colour from the PowerToys Colour Picker screenshot.
        let c = Rgb::parse_hex("#697988").unwrap();
        assert_eq!(c, Rgb(105, 121, 136));
        assert_eq!(c.format("hex").unwrap(), "#697988");
        assert_eq!(c.format("hsl").unwrap(), "hsl(209, 13%, 47%)");
        assert_eq!(c.format("hsv").unwrap(), "hsv(209, 23%, 53%)");
        assert_eq!(c.format("cmyk").unwrap(), "cmyk(23%, 11%, 0%, 47%)");
    }

    #[test]
    fn handles_greys_and_bad_input() {
        assert_eq!(Rgb(0, 0, 0).format("cmyk").unwrap(), "cmyk(0%, 0%, 0%, 100%)");
        assert_eq!(Rgb(128, 128, 128).format("hsl").unwrap(), "hsl(0, 0%, 50%)");
        assert_eq!(Rgb::parse_hex("#12345"), None);
        assert_eq!(Rgb(1, 2, 3).format("lab"), None);
    }

    #[test]
    fn shades_towards_white_and_black() {
        assert_eq!(Rgb(100, 100, 100).shade(1.0), Rgb(255, 255, 255));
        assert_eq!(Rgb(100, 100, 100).shade(-0.5), Rgb(50, 50, 50));
    }
}
