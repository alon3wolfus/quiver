//! Full-screen overlays over a frozen screenshot, shared by the screen ruler and
//! ZoomIt-style drawing.

use crate::util;
use anyhow::{Context, Result};
use gtk::cairo;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use image::RgbaImage;
use std::process::Command;
use std::rc::Rc;

/// A screenshot of the focused monitor, taken before the overlay appears.
pub struct Frozen {
    pub image: RgbaImage,
    /// Monitor scale: screenshot pixels per overlay (logical) pixel.
    pub scale: f64,
    pub monitor: String,
}

pub fn capture_focused() -> Result<Frozen> {
    let monitors = util::hypr_query("monitors")?;
    let focused = monitors
        .as_array()
        .and_then(|m| m.iter().find(|m| m["focused"].as_bool() == Some(true)))
        .context("no focused monitor")?;
    let monitor = focused["name"].as_str().context("monitor has no name")?.to_string();
    let scale = focused["scale"].as_f64().unwrap_or(1.0);
    let png = Command::new("grim").args(["-o", &monitor, "-"]).output().context("failed to start `grim`")?;
    if !png.status.success() {
        anyhow::bail!("grim failed: {}", String::from_utf8_lossy(&png.stderr).trim());
    }
    let image = image::load_from_memory(&png.stdout).context("couldn't read the screenshot")?.to_rgba8();
    Ok(Frozen { image, scale, monitor })
}

/// The screenshot as a cairo surface (cairo wants premultiplied BGRA).
pub fn surface(image: &RgbaImage) -> Result<cairo::ImageSurface> {
    let (w, h) = image.dimensions();
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w as i32, h as i32)?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data()?;
        for (x, y, p) in image.enumerate_pixels() {
            let i = y as usize * stride + x as usize * 4;
            data[i] = p[2];
            data[i + 1] = p[1];
            data[i + 2] = p[0];
            data[i + 3] = 255;
        }
    }
    Ok(surface)
}

/// Paints the frozen screenshot so it exactly covers the overlay.
pub fn paint_frozen(cr: &cairo::Context, surface: &cairo::ImageSurface, scale: f64) {
    cr.save().ok();
    cr.scale(1.0 / scale, 1.0 / scale);
    cr.set_source_surface(surface, 0.0, 0.0).ok();
    cr.paint().ok();
    cr.restore().ok();
}

/// A text label in a dark rounded box, used for measurements and hints.
pub fn pill(cr: &cairo::Context, text: &str, x: f64, y: f64, size: f64) {
    cr.set_font_size(size);
    let Ok(ext) = cr.text_extents(text) else { return };
    let (pad_x, pad_y) = (10.0, 6.0);
    let (w, h) = (ext.width() + 2.0 * pad_x, size + 2.0 * pad_y);
    let r = h / 2.0;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
    cr.arc(x + r, y + r, r, std::f64::consts::FRAC_PI_2, 3.0 * std::f64::consts::FRAC_PI_2);
    cr.close_path();
    cr.set_source_rgba(0.08, 0.1, 0.14, 0.85);
    cr.fill().ok();
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.move_to(x + pad_x - ext.x_bearing(), y + pad_y + size * 0.8);
    cr.show_text(text).ok();
}

/// A borderless full-screen layer on the given monitor that takes the keyboard.
pub fn window(app: &gtk::Application, monitor: &str) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder().application(app).build();
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_namespace(Some("quiver-overlay"));
    window.set_exclusive_zone(-1);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    // Put it on the monitor the screenshot came from.
    if let Some(display) = gtk::gdk::Display::default() {
        let monitors = display.monitors();
        for i in 0..monitors.n_items() {
            if let Some(m) = monitors.item(i).and_downcast::<gtk::gdk::Monitor>() {
                if m.connector().as_deref() == Some(monitor) {
                    window.set_monitor(Some(&m));
                }
            }
        }
    }
    window
}

/// Takes the screenshot, then runs a one-off GTK app showing `build`'s overlay.
pub fn run(app_id: &str, build: impl Fn(&gtk::Application, Rc<Frozen>) + 'static) -> Result<()> {
    let frozen = Rc::new(capture_focused()?);
    let app = gtk::Application::builder()
        .application_id(app_id)
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| build(app, frozen.clone()));
    app.run_with_args::<&str>(&[]);
    Ok(())
}
