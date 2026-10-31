use crate::color::{self, FORMATS, Rgb};
use crate::util;
use anyhow::{Context, Result, bail};

/// Lets you click a pixel and returns its colour, or None if you pressed Escape.
pub fn pick_colour() -> Result<Option<Rgb>> {
    // hyprpicker freezes the screen, shows a zoom lens and prints the clicked colour.
    // No --quiet: hyprpicker counts the colour itself as a log line and would hide it.
    let out = util::output("hyprpicker", &["--format", "hex", "--no-fancy"])?;
    parse_output(&out)
}

/// The picked colour from hyprpicker's output, skipping any log lines around it.
/// Empty output means Escape was pressed.
fn parse_output(out: &str) -> Result<Option<Rgb>> {
    if out.trim().is_empty() {
        return Ok(None);
    }
    let colour = out
        .lines()
        .rev()
        .find_map(|line| Rgb::parse_hex(line.trim()))
        .with_context(|| format!("unexpected hyprpicker output: {}", out.trim()))?;
    color::remember(colour).context("couldn't save the colour history")?;
    Ok(Some(colour))
}

pub fn pick(format: &str) -> Result<()> {
    if !FORMATS.contains(&format) {
        bail!("unknown format `{format}`, use one of: {}", FORMATS.join(", "));
    }
    let Some(colour) = pick_colour()? else { return Ok(()) };
    let text = colour.format(format).unwrap();
    util::copy(&text)?;
    util::notify("Colour copied", &text);
    println!("{text}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_colour_among_log_lines() {
        // Keep the test from writing to the real colour history.
        let dir = std::env::temp_dir().join(format!("quiver-picker-{}", std::process::id()));
        unsafe { std::env::set_var("XDG_DATA_HOME", &dir) };
        let out = "[LOG] Picking colour\n#697988\n";
        assert_eq!(parse_output(out).unwrap(), Some(Rgb(105, 121, 136)));
        assert_eq!(parse_output("  \n").unwrap(), None);
        assert!(parse_output("[ERR] couldn't open display").is_err());
        assert_eq!(color::history().first(), Some(&Rgb(105, 121, 136)));
        let _ = std::fs::remove_dir_all(dir);
    }
}
