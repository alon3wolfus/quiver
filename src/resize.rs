use anyhow::{Context, Result, bail};
use image::ImageFormat;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use std::path::{Path, PathBuf};

/// A target size: fit inside a box, or scale by a percentage.
#[derive(Debug, PartialEq)]
pub enum Size {
    Fit { w: u32, h: u32, label: String },
    Percent(u32),
}

/// Parses "medium", "1280x720" or "50%". Presets match PowerToys Image Resizer.
pub fn parse_size(text: &str) -> Result<Size> {
    let preset = |w, h, label: &str| Ok(Size::Fit { w, h, label: label.into() });
    match text.to_lowercase().as_str() {
        "small" => return preset(854, 480, "Small"),
        "medium" => return preset(1366, 768, "Medium"),
        "large" => return preset(1920, 1080, "Large"),
        "phone" => return preset(320, 568, "Phone"),
        _ => {}
    }
    if let Some(pct) = text.strip_suffix('%') {
        let pct: u32 = pct.parse().context("percent must be a whole number, e.g. 50%")?;
        if pct == 0 {
            bail!("percent must be above 0");
        }
        return Ok(Size::Percent(pct));
    }
    let (w, h) = text
        .split_once(['x', 'X'])
        .context("size must be small, medium, large, phone, WIDTHxHEIGHT or N%")?;
    let (w, h): (u32, u32) = (w.parse()?, h.parse()?);
    if w == 0 || h == 0 {
        bail!("width and height must be above 0");
    }
    Ok(Size::Fit { w, h, label: text.to_string() })
}

/// The new pixel size, keeping the aspect ratio. Fit sizes never enlarge an image.
fn target(size: &Size, w: u32, h: u32) -> (u32, u32) {
    let scale = match size {
        Size::Percent(pct) => *pct as f64 / 100.0,
        Size::Fit { w: bw, h: bh, .. } => (*bw as f64 / w as f64).min(*bh as f64 / h as f64).min(1.0),
    };
    let px = |v: u32| ((v as f64 * scale).round() as u32).max(1);
    (px(w), px(h))
}

/// "photo.jpg" → "photo (Medium).jpg", next to the original.
fn output_path(path: &Path, size: &Size) -> PathBuf {
    let label = match size {
        Size::Fit { label, .. } => label.clone(),
        Size::Percent(pct) => format!("{pct}%"),
    };
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let name = match path.extension() {
        Some(ext) => format!("{stem} ({label}).{}", ext.to_string_lossy()),
        None => format!("{stem} ({label})"),
    };
    path.with_file_name(name)
}

pub fn resize_one(path: &Path, size: &Size, overwrite: bool) -> Result<String> {
    let img = image::open(path).with_context(|| format!("can't open {}", path.display()))?;
    let (w, h) = target(size, img.width(), img.height());
    if (w, h) == (img.width(), img.height()) {
        return Ok(format!("{}: already {w}x{h}, skipped", path.display()));
    }
    let out = if overwrite { path.to_path_buf() } else { output_path(path, size) };
    let resized = img.resize_exact(w, h, FilterType::Lanczos3);

    let format = ImageFormat::from_path(&out).unwrap_or(ImageFormat::Png);
    if format == ImageFormat::Jpeg {
        // The default JPEG quality (75) looks rough; PowerToys uses 90.
        let file = std::fs::File::create(&out)?;
        resized.to_rgb8().write_with_encoder(JpegEncoder::new_with_quality(file, 90))?;
    } else {
        resized.save_with_format(&out, format)?;
    }
    Ok(format!("{}: {}x{} → {w}x{h}  {}", path.display(), img.width(), img.height(), out.display()))
}

pub fn run(files: &[PathBuf], size: &str, overwrite: bool) -> Result<()> {
    let size = parse_size(size)?;
    let mut failed = 0;
    for path in files {
        match resize_one(path, &size, overwrite) {
            Ok(line) => println!("{line}"),
            Err(err) => {
                eprintln!("{}: {err:#}", path.display());
                failed += 1;
            }
        }
    }
    if failed > 0 {
        bail!("{failed} of {} image(s) failed", files.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("Medium").unwrap(), Size::Fit { w: 1366, h: 768, label: "Medium".into() });
        assert_eq!(parse_size("50%").unwrap(), Size::Percent(50));
        assert!(matches!(parse_size("800x600").unwrap(), Size::Fit { w: 800, h: 600, .. }));
        assert!(parse_size("huge").is_err());
        assert!(parse_size("0x10").is_err());
    }

    #[test]
    fn keeps_aspect_and_never_enlarges() {
        let medium = parse_size("medium").unwrap();
        assert_eq!(target(&medium, 4000, 3000), (1024, 768));
        assert_eq!(target(&medium, 640, 480), (640, 480));
        assert_eq!(target(&Size::Percent(50), 640, 480), (320, 240));
    }

    #[test]
    fn names_output_like_powertoys() {
        let out = output_path(Path::new("pics/cat.png"), &parse_size("small").unwrap());
        assert_eq!(out, PathBuf::from("pics/cat (Small).png"));
    }

    #[test]
    fn resizes_a_real_file() {
        let dir = std::env::temp_dir().join(format!("quiver-resize-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("big.jpg");
        image::RgbImage::new(2000, 1000).save(&src).unwrap();
        run(&[src], "small", false).unwrap();
        let out = image::open(dir.join("big (Small).jpg")).unwrap();
        assert_eq!((out.width(), out.height()), (854, 427));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
