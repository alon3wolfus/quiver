use super::*;
use crate::{util, zones};

/// A small drawing of a layout's zones, like the FancyZones editor cards.
fn preview(zones: Vec<[f64; 4]>) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder().content_width(176).content_height(100).build();
    area.set_draw_func(move |area, cr, w, h| {
        let fg = area.color();
        let (r, g, b) = (fg.red() as f64, fg.green() as f64, fg.blue() as f64);
        let (w, h, gap) = (w as f64, h as f64, 3.0);
        for zone in &zones {
            let x = zone[0] / 100.0 * w + gap / 2.0;
            let y = zone[1] / 100.0 * h + gap / 2.0;
            let zw = (zone[2] / 100.0 * w - gap).max(1.0);
            let zh = (zone[3] / 100.0 * h - gap).max(1.0);
            cr.set_source_rgba(r, g, b, 0.16);
            cr.rectangle(x, y, zw, zh);
            let _ = cr.fill();
            cr.set_source_rgba(r, g, b, 0.35);
            cr.set_line_width(1.0);
            cr.rectangle(x + 0.5, y + 0.5, zw - 1.0, zh - 1.0);
            let _ = cr.stroke();
        }
    });
    area
}

fn fill(grid: &gtk::FlowBox, status: &gtk::Label) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
    match zones::layouts() {
        Ok(layouts) => {
            for (name, zones) in layouts {
                let card = card();
                card.add_css_class("layout-card");
                card.set_spacing(10);
                let title = hbox(8);
                let name_label = label(&name, &["row-title"]);
                name_label.set_hexpand(true);
                title.append(&name_label);
                let count = if zones.len() == 1 { "1 zone".to_string() } else { format!("{} zones", zones.len()) };
                title.append(&label(&count, &["row-subtitle"]));
                card.append(&title);
                card.append(&preview(zones));
                card.set_widget_name(&name);
                grid.insert(&card, -1);
            }
        }
        Err(err) => status.set_label(&format!("{err:#}")),
    }
}

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = page(ctx, "zones", &["zones"]);

    let head = hbox(12);
    let title = label("Layouts", &["section-title"]);
    title.set_hexpand(true);
    head.append(&title);
    let edit = button("Edit layouts", Some("document-edit-symbolic"), "accent");
    head.append(&edit);
    page.append(&head);
    page.append(&label(
        "Click a layout to tile the windows on your current workspace into it. The most recently used window gets the first zone.",
        &["row-subtitle"],
    ));

    let grid = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(5)
        .min_children_per_line(2)
        .row_spacing(12)
        .column_spacing(12)
        .homogeneous(true)
        .activate_on_single_click(true)
        .build();
    page.append(&grid);
    let status = label("", &["row-subtitle"]);
    page.append(&status);

    {
        let status = status.clone();
        grid.connect_child_activated(move |_, child| {
            let Some(name) = child.child().map(|c| c.widget_name().to_string()) else { return };
            match zones::apply(&name, false) {
                Ok(()) => status.set_label(&format!("Tiled your windows into “{name}”.")),
                Err(err) => status.set_label(&format!("{err:#}")),
            }
        });
    }
    {
        let status = status.clone();
        edit.connect_clicked(move |_| {
            let result = zones::ensure_config()
                .and_then(|path| util::spawn(&format!("xdg-open {}", util::sh_quote(&path.to_string_lossy()))).map(|_| path));
            match result {
                Ok(path) => status.set_label(&format!("Opened {}. Changes show up when you come back to this page.", path.display())),
                Err(err) => status.set_label(&format!("{err:#}")),
            }
        });
    }
    // Re-read the layouts file every time the page is shown, so edits appear.
    {
        let (grid, status) = (grid.clone(), status.clone());
        page.connect_map(move |_| fill(&grid, &status));
    }
    fill(&grid, &status);
    page
}
