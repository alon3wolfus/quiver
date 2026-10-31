use crate::util;
use anyhow::{Context, Result, bail};
use std::process::{Command, Stdio};

/// Lets you drag a box over the screen and returns the text in it,
/// or None if you pressed Escape.
pub fn capture(lang: &str) -> Result<Option<String>> {
    // slurp lets you drag a rectangle and prints it as "x,y wxh".
    let region = match util::output("slurp", &[]) {
        Ok(region) => region,
        Err(_) => return Ok(None),
    };

    // grim screenshots that region as PNG to stdout; tesseract reads it from stdin.
    let grim = Command::new("grim")
        .args(["-g", &region, "-"])
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to start `grim`")?;
    let out = Command::new("tesseract")
        .args(["stdin", "stdout", "-l", lang])
        .stdin(grim.stdout.unwrap())
        .stderr(Stdio::piped())
        .output()
        .context("failed to start `tesseract`")?;
    if !out.status.success() {
        bail!(
            "tesseract failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(Some(String::from_utf8_lossy(&out.stdout).trim().to_string()))
}

/// Installed tesseract languages, e.g. ["eng", "deu"].
pub fn languages() -> Vec<String> {
    util::output("tesseract", &["--list-langs"])
        .unwrap_or_default()
        .lines()
        .skip(1) // "List of available languages in ..."
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "osd") // osd only detects orientation
        .map(String::from)
        .collect()
}

pub fn extract(lang: &str) -> Result<()> {
    let Some(text) = capture(lang)? else { return Ok(()) };
    if text.is_empty() {
        util::notify("No text found", "Try selecting a larger area");
        return Ok(());
    }
    util::copy(&text)?;
    let preview: String = text.chars().take(80).collect();
    util::notify("Text copied", &preview);
    println!("{text}");
    Ok(())
}
