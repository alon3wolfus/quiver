use crate::util;
use anyhow::{Context, Result, bail};
use std::process::{Command, Stdio};

pub fn extract(lang: &str) -> Result<()> {
    // slurp lets you drag a rectangle and prints it as "x,y wxh".
    let region = match util::output("slurp", &[]) {
        Ok(region) => region,
        Err(_) => return Ok(()), // cancelled with Escape
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

    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
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
