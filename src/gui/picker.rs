use super::*;
use crate::color::{self, Rgb};
use crate::picker;
use std::cell::Cell;

/// Lighter → the colour itself → darker, like the PowerToys shade strip.
const SHADES: [f64; 5] = [0.5, 0.25, 0.0, -0.25, -0.5];
const VALUES: [(&str, &str); 5] = [("HEX", "hex"), ("RGB", "rgb"), ("HSL", "hsl"), ("HSV", "hsv"), ("CMYK", "cmyk")];

fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

struct State {
    current: Cell<Option<Rgb>>,
    values: Vec<(gtk::Label, &'static str)>,
    strip: gtk::DrawingArea,
    history: gtk::Box,
}

impl State {
    fn select(self: &Rc<Self>, colour: Rgb) {
        self.current.set(Some(colour));
        self.refresh();
    }

    fn refresh(self: &Rc<Self>) {
        let current = self.current.get();
        for (value, format) in &self.values {
            value.set_label(&current.and_then(|c| c.format(format)).unwrap_or_else(|| "—".into()));
        }
        self.strip.queue_draw();

        // Recent colours as round swatches; the selected one gets a ring.
        while let Some(child) = self.history.first_child() {
            self.history.remove(&child);
        }
        let colours = color::history();
        if colours.is_empty() {
            let hint = label("Colours you pick show up here.", &["dim"]);
            hint.set_wrap(false);
            self.history.append(&hint);
        }
        for colour in colours {
            let swatch = gtk::DrawingArea::builder().content_width(30).content_height(30).valign(gtk::Align::Center).build();
            let selected = current == Some(colour);
            swatch.set_draw_func(move |area, cr, w, h| {
                let (cx, cy, radius) = (w as f64 / 2.0, h as f64 / 2.0, w.min(h) as f64 / 2.0 - 1.0);
                if selected {
                    let fg = area.color();
                    cr.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, 0.9);
                    cr.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
                    let _ = cr.fill();
                }
                let (r, g, b) = colour.unit();
                cr.set_source_rgb(r, g, b);
                cr.arc(cx, cy, if selected { radius - 3.0 } else { radius }, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            });
            swatch.set_tooltip_text(colour.format("hex").as_deref());
            let click = gtk::GestureClick::new();
            let state = self.clone();
            click.connect_released(move |_, _, _, _| state.select(colour));
            swatch.add_controller(click);
            self.history.append(&swatch);
        }
    }
}

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = page(ctx, "picker", &["pick"]);
    let card = card();
    card.set_spacing(16);

    // Top row: Pick button and recent colours.
    let top = hbox(14);
    let pick = button("Pick", Some("color-select-symbolic"), "soft");
    top.append(&pick);
    let history = hbox(10);
    history.set_hexpand(true);
    top.append(&history);
    card.append(&top);
    card.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // Bottom: shade strip on the left, colour values on the right.
    let body = hbox(18);
    // No fixed height: the strip stretches to match the value rows beside it.
    let strip = gtk::DrawingArea::builder().content_width(56).build();
    body.append(&strip);
    let values_box = vbox(10);
    values_box.set_hexpand(true);
    let mut values = Vec::new();
    for (name, format) in VALUES {
        let row = hbox(12);
        row.add_css_class("value-row");
        row.append(&gtk::Label::builder().label(name).xalign(0.0).css_classes(["value-name"]).build());
        let value = gtk::Label::builder().label("—").xalign(0.0).hexpand(true).selectable(true).css_classes(["value"]).build();
        row.append(&value);
        let copy = icon_button("edit-copy-symbolic", &format!("Copy {name}"));
        {
            let value = value.clone();
            copy.connect_clicked(move |b| {
                if value.label() != "—" {
                    b.clipboard().set_text(&value.label());
                }
            });
        }
        row.append(&copy);
        values_box.append(&row);
        values.push((value, format));
    }
    body.append(&values_box);
    card.append(&body);
    let status = label("", &["dim"]);
    card.append(&status);
    page.append(&card);

    let state = Rc::new(State { current: Cell::new(color::history().first().copied()), values, strip: strip.clone(), history });

    {
        let state = state.clone();
        strip.set_draw_func(move |area, cr, w, h| {
            let (w, h) = (w as f64, h as f64);
            rounded_rect(cr, 0.0, 0.0, w, h, 8.0);
            cr.clip();
            let band = h / SHADES.len() as f64;
            for (i, amount) in SHADES.iter().enumerate() {
                match state.current.get() {
                    Some(c) => {
                        let (r, g, b) = c.shade(*amount).unit();
                        cr.set_source_rgb(r, g, b);
                    }
                    // Nothing picked yet: faint steps of the text colour.
                    None => {
                        let fg = area.color();
                        cr.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, 0.06 + 0.05 * i as f64);
                    }
                }
                cr.rectangle(0.0, band * i as f64, w, band + 1.0);
                let _ = cr.fill();
            }
        });
    }
    // Clicking a shade selects it.
    {
        let click = gtk::GestureClick::new();
        let (state, area) = (state.clone(), strip.clone());
        click.connect_released(move |_, _, _, y| {
            let Some(colour) = state.current.get() else { return };
            let band = ((y / area.height() as f64) * SHADES.len() as f64) as usize;
            if let Some(amount) = SHADES.get(band) {
                state.select(colour.shade(*amount));
            }
        });
        strip.add_controller(click);
    }
    {
        let (state, window, status) = (state.clone(), ctx.window.clone(), status.clone());
        pick.connect_clicked(move |_| {
            let (state, status) = (state.clone(), status.clone());
            run_hidden(&window, picker::pick_colour, move |result| match result {
                Ok(Some(colour)) => {
                    state.select(colour);
                    // Like PowerToys, picking copies the colour straight away.
                    let hex = colour.format("hex").unwrap_or_default();
                    status.clipboard().set_text(&hex);
                    status.set_label(&format!("Copied {hex}"));
                }
                Ok(None) => {}
                Err(err) => status.set_label(&format!("{err:#}")),
            });
        });
    }
    state.refresh();
    page
}
