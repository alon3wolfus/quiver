//! ZoomIt-style drawing: annotate a frozen screenshot, then copy or save it.
//! Drag to draw, Shift for straight lines, Ctrl for boxes.

use crate::overlay::{self, Frozen};
use crate::util;
use anyhow::{Context, Result};
use gtk::cairo;
use gtk::prelude::*;
use gtk::{gdk, glib};
use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;

/// Keys that pick a colour, like ZoomIt (R, G, B, Y, O, P, W, K).
const COLOURS: [(gdk::Key, (f64, f64, f64)); 8] = [
    (gdk::Key::r, (0.95, 0.20, 0.25)),
    (gdk::Key::g, (0.20, 0.80, 0.35)),
    (gdk::Key::b, (0.15, 0.50, 1.00)),
    (gdk::Key::y, (1.00, 0.85, 0.10)),
    (gdk::Key::o, (1.00, 0.55, 0.10)),
    (gdk::Key::p, (0.95, 0.40, 0.80)),
    (gdk::Key::w, (1.00, 1.00, 1.00)),
    (gdk::Key::k, (0.05, 0.05, 0.05)),
];

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Pen,
    Line,
    Rect,
}

#[derive(Clone)]
struct Stroke {
    shape: Shape,
    colour: (f64, f64, f64),
    width: f64,
    points: Vec<(f64, f64)>,
}

struct State {
    strokes: Vec<Stroke>,
    current: Option<Stroke>,
    colour: (f64, f64, f64),
    width: f64,
    message: String,
}

fn draw_stroke(cr: &cairo::Context, s: &Stroke) {
    let (r, g, b) = s.colour;
    cr.set_source_rgb(r, g, b);
    cr.set_line_width(s.width);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    let (Some(first), Some(last)) = (s.points.first(), s.points.last()) else { return };
    match s.shape {
        Shape::Pen => {
            cr.move_to(first.0, first.1);
            for p in &s.points[1..] {
                cr.line_to(p.0, p.1);
            }
            if s.points.len() == 1 {
                cr.line_to(first.0 + 0.1, first.1); // a dot
            }
        }
        Shape::Line => {
            cr.move_to(first.0, first.1);
            cr.line_to(last.0, last.1);
        }
        Shape::Rect => cr.rectangle(first.0.min(last.0), first.1.min(last.1), (last.0 - first.0).abs(), (last.1 - first.1).abs()),
    }
    cr.stroke().ok();
}

/// The screenshot with the drawing on top, at full screen resolution, as PNG.
fn export(frozen: &Frozen, surface: &cairo::ImageSurface, strokes: &[Stroke]) -> Result<Vec<u8>> {
    let (w, h) = frozen.image.dimensions();
    let out = cairo::ImageSurface::create(cairo::Format::ARgb32, w as i32, h as i32)?;
    {
        let cr = cairo::Context::new(&out)?;
        cr.set_source_surface(surface, 0.0, 0.0)?;
        cr.paint()?;
        // Strokes are in overlay (logical) pixels; scale them up to screenshot pixels.
        cr.scale(frozen.scale, frozen.scale);
        for s in strokes {
            draw_stroke(&cr, s);
        }
    }
    let mut png = Vec::new();
    out.write_to_png(&mut png)?;
    Ok(png)
}

fn copy_png(png: &[u8]) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .args(["--type", "image/png"])
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to start `wl-copy`")?;
    child.stdin.take().unwrap().write_all(png)?;
    child.wait()?;
    Ok(())
}

fn save_png(png: &[u8]) -> Result<PathBuf> {
    let pictures = util::output("xdg-user-dir", &["PICTURES"])
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Pictures"));
    let stamp = util::output("date", &["+%Y-%m-%d_%H-%M-%S"]).unwrap_or_else(|_| "drawing".into());
    std::fs::create_dir_all(&pictures)?;
    let path = pictures.join(format!("quiver_{stamp}.png"));
    std::fs::write(&path, png)?;
    Ok(path)
}

fn build(app: &gtk::Application, frozen: Rc<Frozen>) {
    let window = overlay::window(app, &frozen.monitor);
    let area = gtk::DrawingArea::new();
    window.set_child(Some(&area));
    window.set_cursor_from_name(Some("crosshair"));
    let surface = overlay::surface(&frozen.image).expect("screenshot surface");
    let state = Rc::new(RefCell::new(State {
        strokes: Vec::new(),
        current: None,
        colour: COLOURS[0].1,
        width: 4.0,
        message: String::new(),
    }));

    {
        let (state, frozen, surface) = (state.clone(), frozen.clone(), surface.clone());
        area.set_draw_func(move |_, cr, _, _| {
            overlay::paint_frozen(cr, &surface, frozen.scale);
            let st = state.borrow();
            for s in st.strokes.iter().chain(st.current.iter()) {
                draw_stroke(cr, s);
            }
            let hint = if st.message.is_empty() {
                "Drag to draw  ·  Shift line  ·  Ctrl box  ·  R G B Y O P W K colour  ·  +/− size  ·  Ctrl+Z undo  ·  E clear  ·  Ctrl+C copy  ·  Ctrl+S save  ·  Esc close"
                    .to_string()
            } else {
                st.message.clone()
            };
            overlay::pill(cr, &hint, 20.0, 20.0, 13.0);
            // Current colour and size.
            let (r, g, b) = st.colour;
            cr.set_source_rgb(r, g, b);
            cr.arc(32.0, 64.0, st.width.max(3.0), 0.0, std::f64::consts::TAU);
            cr.fill().ok();
        });
    }

    let drag = gtk::GestureDrag::new();
    {
        let state = state.clone();
        drag.connect_drag_begin(move |gesture, x, y| {
            let mods = gesture.current_event_state();
            let shape = if mods.contains(gdk::ModifierType::CONTROL_MASK) {
                Shape::Rect
            } else if mods.contains(gdk::ModifierType::SHIFT_MASK) {
                Shape::Line
            } else {
                Shape::Pen
            };
            let mut st = state.borrow_mut();
            let (colour, width) = (st.colour, st.width);
            st.current = Some(Stroke { shape, colour, width, points: vec![(x, y)] });
            st.message.clear();
        });
    }
    {
        let (state, area) = (state.clone(), area.clone());
        drag.connect_drag_update(move |gesture, dx, dy| {
            let Some((x0, y0)) = gesture.start_point() else { return };
            let mut st = state.borrow_mut();
            if let Some(stroke) = st.current.as_mut() {
                let p = (x0 + dx, y0 + dy);
                if stroke.shape == Shape::Pen {
                    stroke.points.push(p);
                } else {
                    stroke.points.truncate(1);
                    stroke.points.push(p);
                }
            }
            drop(st);
            area.queue_draw();
        });
    }
    {
        let (state, area) = (state.clone(), area.clone());
        drag.connect_drag_end(move |_, _, _| {
            let mut st = state.borrow_mut();
            if let Some(stroke) = st.current.take() {
                st.strokes.push(stroke);
            }
            drop(st);
            area.queue_draw();
        });
    }
    area.add_controller(drag);

    let keys = gtk::EventControllerKey::new();
    {
        let (app, state, area) = (app.clone(), state.clone(), area.clone());
        keys.connect_key_pressed(move |_, key, _, mods| {
            let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
            let mut st = state.borrow_mut();
            match key {
                gdk::Key::Escape => {
                    app.quit();
                    return glib::Propagation::Stop;
                }
                gdk::Key::z if ctrl => {
                    st.strokes.pop();
                }
                gdk::Key::c | gdk::Key::s if ctrl => {
                    let png = export(&frozen, &surface, &st.strokes);
                    st.message = match (key, png) {
                        (_, Err(err)) => format!("Couldn't export: {err:#}"),
                        (gdk::Key::c, Ok(png)) => match copy_png(&png) {
                            Ok(()) => "Copied the drawing to the clipboard".into(),
                            Err(err) => format!("{err:#}"),
                        },
                        (_, Ok(png)) => match save_png(&png) {
                            Ok(path) => format!("Saved to {}", path.display()),
                            Err(err) => format!("{err:#}"),
                        },
                    };
                }
                gdk::Key::e => st.strokes.clear(),
                gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => st.width = (st.width + 2.0).min(40.0),
                gdk::Key::minus | gdk::Key::KP_Subtract => st.width = (st.width - 2.0).max(2.0),
                _ => match COLOURS.iter().find(|(k, _)| *k == key.to_lower()) {
                    Some((_, colour)) if !ctrl => st.colour = *colour,
                    _ => return glib::Propagation::Proceed,
                },
            }
            drop(st);
            area.queue_draw();
            glib::Propagation::Stop
        });
    }
    window.add_controller(keys);
    window.present();
}

pub fn run() -> Result<()> {
    overlay::run("dev.quiver.draw", build)
}
