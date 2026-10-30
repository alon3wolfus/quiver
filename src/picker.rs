use crate::util;
use anyhow::{Result, bail};

const FORMATS: [&str; 5] = ["hex", "rgb", "hsl", "hsv", "cmyk"];

pub fn pick(format: &str) -> Result<()> {
    if !FORMATS.contains(&format) {
        bail!("unknown format `{format}`, use one of: {}", FORMATS.join(", "));
    }
    // hyprpicker freezes the screen, shows a zoom lens and prints the clicked colour.
    let colour = util::output("hyprpicker", &["--format", format, "--no-fancy"])?;
    if colour.is_empty() {
        return Ok(()); // cancelled with Escape
    }
    util::copy(&colour)?;
    util::notify("Colour copied", &colour);
    println!("{colour}");
    Ok(())
}
