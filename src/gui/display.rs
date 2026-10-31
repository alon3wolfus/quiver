use super::*;
use crate::display::{self, Monitor, Settings};
use std::cell::Cell;

const SCALES: [f64; 9] = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];
const ROTATIONS: [&str; 4] = ["Normal", "90°", "180°", "270°"];
/// Like Windows: new display settings undo themselves unless you confirm them.
const REVERT_AFTER: u32 = 15;

/// The controls for one monitor.
struct Editor {
    original: Settings,
    enabled: gtk::Switch,
    mode: gtk::DropDown,
    modes: Vec<String>,
    scale: gtk::DropDown,
    scales: Vec<f64>,
    rotation: gtk::DropDown,
    x: gtk::SpinButton,
    y: gtk::SpinButton,
}

impl Editor {
    fn settings(&self) -> Settings {
        let rotation = self.rotation.selected() as i64;
        Settings {
            output: self.original.output.clone(),
            mode: self.modes[self.mode.selected() as usize].clone(),
            x: self.x.value() as i64,
            y: self.y.value() as i64,
            scale: self.scales[self.scale.selected() as usize],
            // Keep flipped transforms (4-7) we don't offer in the menu.
            transform: if rotation < ROTATIONS.len() as i64 { rotation } else { self.original.transform },
            disabled: !self.enabled.is_active(),
        }
    }
}

fn dropdown(items: &[String], selected: usize) -> gtk::DropDown {
    let names: Vec<&str> = items.iter().map(String::as_str).collect();
    let dd = gtk::DropDown::from_strings(&names);
    dd.set_selected(selected as u32);
    dd
}

fn editor_card(m: &Monitor, on_change: Rc<dyn Fn()>) -> (gtk::Box, Editor) {
    let card = card();
    card.add_css_class("flush");
    card.set_spacing(0);
    let s = &m.settings;
    let title = label(&format!("{}  ·  {}", m.name, m.description), &["card-title"]);
    title.set_margin_start(16);
    title.set_margin_top(14);
    title.set_margin_bottom(4);
    card.append(&title);

    let enabled = gtk::Switch::builder().active(!s.disabled).build();
    card.append(&setting_row("video-display-symbolic", "Enabled", "Turn this monitor on or off", Some(enabled.upcast_ref())));

    let mut modes = m.modes.clone();
    if !modes.contains(&s.mode) {
        modes.insert(0, s.mode.clone());
    }
    let pretty: Vec<String> = modes.iter().map(|m| m.replace('x', " × ").replace('@', "  @ ") + " Hz").collect();
    let mode = dropdown(&pretty, modes.iter().position(|m| *m == s.mode).unwrap_or(0));
    card.append(&setting_row("zoom-fit-best-symbolic", "Resolution", "Size and refresh rate", Some(mode.upcast_ref())));

    let mut scales = SCALES.to_vec();
    if !scales.iter().any(|v| (v - s.scale).abs() < 0.001) {
        scales.push(s.scale);
        scales.sort_by(f64::total_cmp);
    }
    let scale_names: Vec<String> = scales.iter().map(|v| format!("{}%", (v * 100.0).round())).collect();
    let scale = dropdown(&scale_names, scales.iter().position(|v| (v - s.scale).abs() < 0.001).unwrap_or(2));
    card.append(&setting_row("zoom-in-symbolic", "Scale", "Makes everything on this monitor bigger or smaller", Some(scale.upcast_ref())));

    let mut rotations: Vec<String> = ROTATIONS.iter().map(|r| r.to_string()).collect();
    if s.transform >= ROTATIONS.len() as i64 {
        rotations.push(format!("Flipped ({})", s.transform));
    }
    let rotation = dropdown(&rotations, (s.transform as usize).min(rotations.len() - 1));
    card.append(&setting_row("object-rotate-right-symbolic", "Rotation", "", Some(rotation.upcast_ref())));

    let position = hbox(8);
    let spin = |value: i64| {
        let spin = gtk::SpinButton::with_range(-16384.0, 16384.0, 10.0);
        spin.set_value(value as f64);
        spin
    };
    let (x, y) = (spin(s.x), spin(s.y));
    position.append(&gtk::Label::new(Some("X")));
    position.append(&x);
    position.append(&gtk::Label::new(Some("Y")));
    position.append(&y);
    card.append(&setting_row("view-dual-symbolic", "Position", "Where this monitor sits next to the others, in pixels", Some(position.upcast_ref())));

    // Redraw the layout preview whenever something changes.
    {
        let f = on_change.clone();
        enabled.connect_active_notify(move |_| f());
    }
    for dd in [&mode, &scale, &rotation] {
        let f = on_change.clone();
        dd.connect_selected_notify(move |_| f());
    }
    for sb in [&x, &y] {
        let f = on_change.clone();
        sb.connect_value_changed(move |_| f());
    }

    let editor = Editor { original: s.clone(), enabled, mode, modes, scale, scales, rotation, x, y };
    (card, editor)
}

/// Monitor rectangles in logical (scaled, rotated) size, for the preview.
fn logical_rect(s: &Settings) -> (f64, f64, f64, f64) {
    let (w, h) = s
        .mode
        .split('@')
        .next()
        .and_then(|wh| wh.split_once('x'))
        .and_then(|(w, h)| Some((w.parse::<f64>().ok()?, h.parse::<f64>().ok()?)))
        .unwrap_or((1920.0, 1080.0));
    let (w, h) = if s.transform % 2 == 1 { (h, w) } else { (w, h) };
    (s.x as f64, s.y as f64, w / s.scale, h / s.scale)
}

fn draw_layout(area: &gtk::DrawingArea, cr: &gtk::cairo::Context, w: i32, h: i32, layout: &[Settings]) {
    let shown: Vec<(&Settings, (f64, f64, f64, f64))> =
        layout.iter().filter(|s| !s.disabled).map(|s| (s, logical_rect(s))).collect();
    if shown.is_empty() {
        return;
    }
    let min_x = shown.iter().map(|(_, r)| r.0).fold(f64::MAX, f64::min);
    let min_y = shown.iter().map(|(_, r)| r.1).fold(f64::MAX, f64::min);
    let max_x = shown.iter().map(|(_, r)| r.0 + r.2).fold(f64::MIN, f64::max);
    let max_y = shown.iter().map(|(_, r)| r.1 + r.3).fold(f64::MIN, f64::max);
    let pad = 16.0;
    let k = ((w as f64 - 2.0 * pad) / (max_x - min_x)).min((h as f64 - 2.0 * pad) / (max_y - min_y));
    let (ox, oy) = (
        (w as f64 - (max_x - min_x) * k) / 2.0,
        (h as f64 - (max_y - min_y) * k) / 2.0,
    );
    let fg = area.color();
    let (r, g, b) = (fg.red() as f64, fg.green() as f64, fg.blue() as f64);
    for (s, (x, y, mw, mh)) in shown {
        let (x, y, mw, mh) = (ox + (x - min_x) * k + 2.0, oy + (y - min_y) * k + 2.0, mw * k - 4.0, mh * k - 4.0);
        cr.set_source_rgba(r, g, b, 0.10);
        cr.rectangle(x, y, mw, mh);
        let _ = cr.fill();
        cr.set_source_rgba(r, g, b, 0.45);
        cr.set_line_width(1.5);
        cr.rectangle(x, y, mw, mh);
        let _ = cr.stroke();
        cr.set_source_rgba(r, g, b, 0.9);
        cr.set_font_size(13.0);
        cr.move_to(x + 10.0, y + 22.0);
        let _ = cr.show_text(&s.output);
        cr.set_source_rgba(r, g, b, 0.6);
        cr.set_font_size(11.0);
        cr.move_to(x + 10.0, y + 38.0);
        let _ = cr.show_text(&format!("{}  ·  {}%", s.mode.split('@').next().unwrap_or(""), (s.scale * 100.0).round()));
    }
}

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = page(ctx, "display", &[]);

    let preview = gtk::DrawingArea::builder().content_height(190).hexpand(true).build();
    let preview_card = card();
    preview_card.append(&preview);
    page.append(&preview_card);

    let editors_box = vbox(14);
    page.append(&editors_box);

    // "Keep these settings?" bar shown after applying.
    let confirm = card();
    confirm.set_orientation(gtk::Orientation::Horizontal);
    confirm.set_visible(false);
    let countdown = label("", &["row-title"]);
    countdown.set_hexpand(true);
    countdown.set_valign(gtk::Align::Center);
    let keep = button("Keep changes", None, "accent");
    let revert = button("Revert", None, "soft");
    confirm.append(&countdown);
    confirm.append(&revert);
    confirm.append(&keep);
    page.append(&confirm);

    let actions = hbox(8);
    actions.set_halign(gtk::Align::End);
    let reload = button("Reload", Some("view-refresh-symbolic"), "soft");
    let save = button("Save as default", Some("document-save-symbolic"), "soft");
    let apply = button("Apply", Some("object-select-symbolic"), "accent");
    actions.append(&reload);
    actions.append(&save);
    actions.append(&apply);
    page.append(&actions);
    let status = label("", &["row-subtitle"]);
    status.set_selectable(true);
    page.append(&status);

    // App scaling for GTK apps.
    page.append(&label("App scaling", &["section-title"]));
    let scaling = card();
    scaling.add_css_class("flush");
    scaling.set_spacing(0);
    let text = gtk::SpinButton::with_range(0.5, 3.0, 0.05);
    text.set_digits(2);
    text.set_value(display::text_scale());
    scaling.append(&setting_row(
        "font-x-generic-symbolic",
        "Text size for GTK apps",
        "1.00 is normal. Applies right away to GTK apps like Files and this window.",
        Some(text.upcast_ref()),
    ));
    page.append(&scaling);
    {
        let status = status.clone();
        text.connect_value_changed(move |sb| {
            if let Err(err) = display::set_text_scale(sb.value()) {
                status.set_label(&format!("{err:#}"));
            }
        });
    }

    let editors: Rc<RefCell<Vec<Editor>>> = Rc::default();
    let layout = move |editors: &Rc<RefCell<Vec<Editor>>>| -> Vec<Settings> {
        editors.borrow().iter().map(Editor::settings).collect()
    };
    {
        let editors = editors.clone();
        preview.set_draw_func(move |area, cr, w, h| draw_layout(area, cr, w, h, &layout(&editors)));
    }

    let load = {
        let (editors, editors_box, preview, status) = (editors.clone(), editors_box.clone(), preview.clone(), status.clone());
        Rc::new(move || {
            while let Some(child) = editors_box.first_child() {
                editors_box.remove(&child);
            }
            editors.borrow_mut().clear();
            let monitors = match display::monitors() {
                Ok(m) => m,
                Err(err) => {
                    status.set_label(&format!("{err:#}"));
                    return;
                }
            };
            let redraw: Rc<dyn Fn()> = {
                let preview = preview.clone();
                Rc::new(move || preview.queue_draw())
            };
            for m in &monitors {
                let (card, editor) = editor_card(m, redraw.clone());
                editors_box.append(&card);
                editors.borrow_mut().push(editor);
            }
            preview.queue_draw();
        })
    };

    // Apply → show "Keep changes?" with a countdown; revert unless confirmed.
    let previous: Rc<RefCell<Vec<Settings>>> = Rc::default();
    let pending = Rc::new(Cell::new(0u32));
    let finish = {
        let (confirm, pending, load) = (confirm.clone(), pending.clone(), load.clone());
        Rc::new(move || {
            pending.set(0);
            confirm.set_visible(false);
            load();
        })
    };
    let do_revert = {
        let (previous, status, finish) = (previous.clone(), status.clone(), finish.clone());
        Rc::new(move || {
            match display::apply(&previous.borrow()) {
                Ok(()) => status.set_label("Reverted to the previous display settings."),
                Err(err) => status.set_label(&format!("{err:#}")),
            }
            finish();
        })
    };
    {
        let (editors, previous, status, confirm, countdown, pending, do_revert) = (
            editors.clone(),
            previous.clone(),
            status.clone(),
            confirm.clone(),
            countdown.clone(),
            pending.clone(),
            do_revert.clone(),
        );
        apply.connect_clicked(move |_| {
            let wanted = layout(&editors);
            let before: Vec<Settings> = editors.borrow().iter().map(|e| e.original.clone()).collect();
            if wanted == before {
                status.set_label("Nothing changed.");
                return;
            }
            if let Err(err) = display::apply(&wanted) {
                status.set_label(&format!("{err:#}"));
                return;
            }
            *previous.borrow_mut() = before;
            status.set_label("");
            pending.set(REVERT_AFTER);
            countdown.set_label(&format!("Keep these display settings? Reverting in {REVERT_AFTER} s"));
            confirm.set_visible(true);
            let (pending, countdown, do_revert) = (pending.clone(), countdown.clone(), do_revert.clone());
            glib::timeout_add_seconds_local(1, move || {
                let left = pending.get();
                if left == 0 {
                    return glib::ControlFlow::Break; // kept or reverted already
                }
                if left == 1 {
                    do_revert();
                    return glib::ControlFlow::Break;
                }
                pending.set(left - 1);
                countdown.set_label(&format!("Keep these display settings? Reverting in {} s", left - 1));
                glib::ControlFlow::Continue
            });
        });
    }
    {
        let (status, finish) = (status.clone(), finish.clone());
        keep.connect_clicked(move |_| {
            finish();
            status.set_label("Kept. Use “Save as default” to keep them after a restart.");
        });
    }
    revert.connect_clicked(move |_| do_revert());
    {
        let load = load.clone();
        reload.connect_clicked(move |_| load());
    }
    {
        let (editors, status) = (editors.clone(), status.clone());
        save.connect_clicked(move |_| match display::save(&layout(&editors)) {
            Ok(path) => status.set_label(&format!(
                "Saved to {}. To load it at login, add this line to your Hyprland config:\n{}",
                path.display(),
                display::include_line()
            )),
            Err(err) => status.set_label(&format!("{err:#}")),
        });
    }
    {
        let (load, pending) = (load.clone(), pending.clone());
        // Re-read monitors each time the page is shown, unless a change awaits confirmation.
        page.connect_map(move |_| {
            if pending.get() == 0 {
                load();
            }
        });
    }
    load();
    page
}
