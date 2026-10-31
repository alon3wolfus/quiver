use crate::color::{self, FORMATS, Rgb};
use crate::util;
use anyhow::{Context, Result, bail};

/// Lets you click a pixel and returns its colour, or None if you pressed Escape.
pub fn pick_colour() -> Result<Option<Rgb>> {
    // hyprpicker freezes the screen, shows a zoom lens and prints the clicked colour.
    let out = util::output("hyprpicker", &["--format", "hex", "--no-fancy", "--quiet"])?;
    let Some(line) = out.lines().rev().find(|l| !l.trim().is_empty()) else {
        return Ok(None);
    };
    let colour = Rgb::parse_hex(line).with_context(|| format!("unexpected hyprpicker output: {line}"))?;
    let _ = color::remember(colour);
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
