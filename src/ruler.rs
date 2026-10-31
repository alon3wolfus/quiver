//! Screen Ruler: measure distances on a frozen screenshot. Moving the mouse shows the
//! distance to the nearest colour edge in each direction; dragging measures a box.

use crate::overlay::{self, Frozen};
use crate::util;
use anyhow::Result;
use gtk::prelude::*;
use gtk::{gdk, glib};
use image::RgbaImage;
use std::cell::RefCell;
use std::rc::Rc;

/// How different two colours must be to count as an edge (sum of RGB differences).
const TOLERANCE: u32 = 30;
const LINE: (f64, f64, f64) = (1.0, 0.25, 0.45);

fn differs(a: &image::Rgba<u8>, b: &image::Rgba<u8>) -> bool {
    let d = |i: usize| (a[i] as i32 - b[i] as i32).unsigned_abs();
    d(0) + d(1) + d(2) > TOLERANCE
}

/// The run of similar colour around (x, y): (left, right, top, bottom), inclusive.
pub fn edges(img: &RgbaImage, x: u32, y: u32) -> (u32, u32, u32, u32) {
    let (w, h) = img.dimensions();
    let (x, y) = (x.min(w - 1), y.min(h - 1));
    let start = *img.get_pixel(x, y);
    let same = |px: u32, py: u32| !differs(&start, img.get_pixel(px, py));
    let mut left = x;
    while left > 0 && same(left - 1, y) {
        left -= 1;
    }
    let mut right = x;
    while right + 1 < w && same(right + 1, y) {
        right += 1;
    }
    let mut top = y;
    while top > 0 && same(x, top - 1) {
        top -= 1;
    }
    let mut bottom = y;
    while bottom + 1 < h && same(x, bottom + 1) {
        bottom += 1;
    }
    (left, right, top, bottom)
}

#[derive(Default)]
struct State {
    cursor: Option<(f64, f64)>,
    drag: Option<((f64, f64), (f64, f64))>,
    /// Text of the last measurement, copied on click.
    last: String,
    copied: bool,
}

fn build(app: &gtk::Application, frozen: Rc<Frozen>) {
    let window = overlay::window(app, &frozen.monitor);
    let area = gtk::DrawingArea::new();
    window.set_child(Some(&area));
    window.set_cursor_from_name(Some("crosshair"));
    let state = Rc::new(RefCell::new(State::default()));
    let surface = overlay::surface(&frozen.image).expect("screenshot surface");

    {
        let (state, frozen) = (state.clone(), frozen.clone());
        area.set_draw_func(move |_, cr, _, _| {
            let s = frozen.scale;
            overlay::paint_frozen(cr, &surface, s);
            let mut st = state.borrow_mut();
            let (r, g, b) = LINE;
            cr.set_source_rgb(r, g, b);
            cr.set_line_width(1.0);
            let label_at;
            if let Some(((x0, y0), (x1, y1))) = st.drag {
                let (x, y, w, h) = (x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs());
                cr.set_source_rgba(r, g, b, 0.15);
                cr.rectangle(x, y, w, h);
                cr.fill().ok();
                cr.set_source_rgb(r, g, b);
                cr.rectangle(x + 0.5, y + 0.5, w, h);
                cr.stroke().ok();
                st.last = format!("{} × {}", (w * s).round(), (h * s).round());
                label_at = (x1 + 14.0, y1 + 14.0);
            } else if let Some((x, y)) = st.cursor {
                let (left, right, top, bottom) = edges(&frozen.image, (x * s) as u32, (y * s) as u32);
                let (l, rr, t, bb) = (left as f64 / s, (right + 1) as f64 / s, top as f64 / s, (bottom + 1) as f64 / s);
                // Horizontal and vertical lines out to the edges, with end ticks.
                cr.move_to(l, y + 0.5);
                cr.line_to(rr, y + 0.5);
                cr.move_to(x + 0.5, t);
                cr.line_to(x + 0.5, bb);
                for (ax, ay, bx, by) in [(l, y - 5.0, l, y + 5.0), (rr, y - 5.0, rr, y + 5.0), (x - 5.0, t, x + 5.0, t), (x - 5.0, bb, x + 5.0, bb)] {
                    cr.move_to(ax + 0.5, ay + 0.5);
                    cr.line_to(bx + 0.5, by + 0.5);
                }
                cr.stroke().ok();
                st.last = format!("{} × {}", right - left + 1, bottom - top + 1);
                label_at = (x + 14.0, y + 14.0);
            } else {
                label_at = (0.0, 0.0);
            }
            if !st.last.is_empty() && (st.cursor.is_some() || st.drag.is_some()) {
                let text = if st.copied { format!("{} px  ·  copied", st.last) } else { format!("{} px", st.last) };
                overlay::pill(cr, &text, label_at.0, label_at.1, 14.0);
            }
            overlay::pill(cr, "Move to measure  ·  drag for a box  ·  click to copy  ·  Esc to close", 20.0, 20.0, 13.0);
        });
    }

    let motion = gtk::EventControllerMotion::new();
    {
        let (state, area) = (state.clone(), area.clone());
        motion.connect_motion(move |_, x, y| {
            let mut st = state.borrow_mut();
            st.cursor = Some((x, y));
            st.copied = false;
            drop(st);
            area.queue_draw();
        });
    }
    area.add_controller(motion);

    let drag = gtk::GestureDrag::new();
    {
        let state = state.clone();
        drag.connect_drag_begin(move |_, x, y| {
            let mut st = state.borrow_mut();
            st.drag = Some(((x, y), (x, y)));
            st.copied = false;
        });
    }
    {
        let (state, area) = (state.clone(), area.clone());
        drag.connect_drag_update(move |_, dx, dy| {
            let mut st = state.borrow_mut();
            if let Some(((x0, y0), _)) = st.drag {
                st.drag = Some(((x0, y0), (x0 + dx, y0 + dy)));
            }
            drop(st);
            area.queue_draw();
        });
    }
    {
        let (state, area) = (state.clone(), area.clone());
        drag.connect_drag_end(move |_, dx, dy| {
            let mut st = state.borrow_mut();
            // A click (barely moved) copies the edge measurement; a real drag copies the box.
            let was_click = dx.abs() < 3.0 && dy.abs() < 3.0;
            if was_click {
                st.drag = None;
            }
            drop(st);
            area.queue_draw();
            // Copy after the redraw so `last` holds the measurement just shown.
            let state = state.clone();
            let area = area.clone();
            glib::idle_add_local_once(move || {
                let mut st = state.borrow_mut();
                if !st.last.is_empty() && util::copy(&st.last.replace(" × ", "x")).is_ok() {
                    st.copied = true;
                }
                if !was_click {
                    st.drag = None;
                }
                drop(st);
                area.queue_draw();
            });
        });
    }
    area.add_controller(drag);

    let keys = gtk::EventControllerKey::new();
    {
        let app = app.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                app.quit();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    window.add_controller(keys);
    window.present();
}

pub fn run() -> Result<()> {
    overlay::run("dev.quiver.ruler", build)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_edges_of_a_box() {
        // A 10×6 red box at (5, 4) on white.
        let mut img = RgbaImage::from_pixel(30, 20, image::Rgba([255, 255, 255, 255]));
        for y in 4..10 {
            for x in 5..15 {
                img.put_pixel(x, y, image::Rgba([220, 30, 30, 255]));
            }
        }
        assert_eq!(edges(&img, 8, 6), (5, 14, 4, 9));
        // On the white background the run reaches the screen edges, stopping at the box.
        assert_eq!(edges(&img, 20, 6), (15, 29, 0, 19));
    }

    #[test]
    fn small_colour_noise_is_not_an_edge() {
        let mut img = RgbaImage::from_pixel(10, 1, image::Rgba([100, 100, 100, 255]));
        img.put_pixel(6, 0, image::Rgba([105, 103, 101, 255]));
        assert_eq!(edges(&img, 2, 0), (0, 9, 0, 0));
    }
}
