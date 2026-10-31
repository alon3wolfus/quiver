//! quiver's colour tool icons. They're built into the binary, written to the runtime
//! dir on start and added to the icon theme, so `quiver-zones` etc. work as icon names
//! however quiver was installed.

use gtk::gdk;
use std::path::PathBuf;

const ICONS: &[(&str, &str)] = &[
    ("quiver", include_str!("../packaging/quiver.svg")),
    ("quiver-display", include_str!("../icons/quiver-display.svg")),
    ("quiver-draw", include_str!("../icons/quiver-draw.svg")),
    ("quiver-home", include_str!("../icons/quiver-home.svg")),
    ("quiver-launcher", include_str!("../icons/quiver-launcher.svg")),
    ("quiver-locksmith", include_str!("../icons/quiver-locksmith.svg")),
    ("quiver-ocr", include_str!("../icons/quiver-ocr.svg")),
    ("quiver-picker", include_str!("../icons/quiver-picker.svg")),
    ("quiver-record", include_str!("../icons/quiver-record.svg")),
    ("quiver-rename", include_str!("../icons/quiver-rename.svg")),
    ("quiver-resize", include_str!("../icons/quiver-resize.svg")),
    ("quiver-ruler", include_str!("../icons/quiver-ruler.svg")),
    ("quiver-top", include_str!("../icons/quiver-top.svg")),
    ("quiver-zones", include_str!("../icons/quiver-zones.svg")),
    ("quiver-zoomit", include_str!("../icons/quiver-zoomit.svg")),
];

fn dir() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(runtime).join("quiver/icons")
}

/// Makes the icons available to GTK on `display`. Missing icons just fall back to
/// GTK's "image-missing", so errors here aren't fatal.
pub fn install(display: &gdk::Display) {
    let dir = dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    for (name, svg) in ICONS {
        let path = dir.join(format!("{name}.svg"));
        // Rewrite only when changed, so a running GTK doesn't reload for nothing.
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*svg) {
            let _ = std::fs::write(&path, svg);
        }
    }
    gtk::IconTheme::for_display(display).add_search_path(&dir);
}
