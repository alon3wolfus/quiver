use super::*;
use crate::zoomit::{self, ZoomAction};

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = page(ctx, "zoomit", &["zoom", "draw", "record"]);
    let status = label("", &["row-subtitle"]);

    // Zoom
    page.append(&label("Zoom", &["section-title"]));
    let zoom_card = card();
    zoom_card.add_css_class("flush");
    zoom_card.set_spacing(0);
    let zoom_buttons = hbox(6);
    let level = label("", &["row-subtitle"]);
    let out = icon_button("zoom-out-symbolic", "Zoom out");
    let reset = icon_button("zoom-original-symbolic", "Normal size");
    let zin = icon_button("zoom-in-symbolic", "Zoom in");
    zoom_buttons.append(&level);
    zoom_buttons.append(&out);
    zoom_buttons.append(&reset);
    zoom_buttons.append(&zin);
    zoom_card.append(&setting_row(
        "zoom-in-symbolic",
        "Zoom the screen",
        "Hyprland's zoom follows your mouse. Bind `quiver zoom in` / `out` / `toggle` to keys for quick use.",
        Some(zoom_buttons.upcast_ref()),
    ));
    page.append(&zoom_card);
    let show_level = {
        let level = level.clone();
        Rc::new(move |z: f64| level.set_label(&format!("{}%", (z * 100.0).round())))
    };
    show_level(zoomit::zoom_factor());
    for (b, action) in [(&out, ZoomAction::Out), (&reset, ZoomAction::Reset), (&zin, ZoomAction::In)] {
        let (show_level, status) = (show_level.clone(), status.clone());
        b.connect_clicked(move |_| match zoomit::zoom(action) {
            Ok(z) => show_level(z),
            Err(err) => status.set_label(&format!("{err:#}")),
        });
    }

    // Draw
    page.append(&label("Draw", &["section-title"]));
    let draw_card = card();
    draw_card.add_css_class("flush");
    draw_card.set_spacing(0);
    let draw = button("Start drawing", Some("draw-freehand-symbolic"), "accent");
    draw_card.append(&setting_row(
        "draw-freehand-symbolic",
        "Draw on the screen",
        "Freezes the screen so you can annotate it. Shift draws lines, Ctrl draws boxes; R G B Y O P W K change colour; Ctrl+Z undoes; Ctrl+C copies and Ctrl+S saves the result to Pictures.",
        Some(draw.upcast_ref()),
    ));
    page.append(&draw_card);
    {
        let (window, status) = (ctx.window.clone(), status.clone());
        draw.connect_clicked(move |_| {
            let status = status.clone();
            run_quiver_hidden(&window, &["draw"], move |result| {
                if let Err(err) = result {
                    status.set_label(&format!("{err:#}"));
                }
            });
        });
    }

    // Record
    page.append(&label("Record", &["section-title"]));
    let record_card = card();
    record_card.add_css_class("flush");
    record_card.set_spacing(0);
    let record_buttons = hbox(8);
    let whole = button("Record screen", Some("media-record-symbolic"), "accent");
    let region = button("Record region", Some("zoom-fit-best-symbolic"), "soft");
    let stop = button("Stop recording", Some("media-playback-stop-symbolic"), "accent");
    record_buttons.append(&whole);
    record_buttons.append(&region);
    record_buttons.append(&stop);
    record_card.append(&setting_row(
        "media-record-symbolic",
        "Record the screen",
        "Saves an MP4 to your Videos folder with gpu-screen-recorder. Run `quiver record` again (or press Stop) to finish.",
        Some(record_buttons.upcast_ref()),
    ));
    page.append(&record_card);

    let sync = {
        let (whole, region, stop) = (whole.clone(), region.clone(), stop.clone());
        Rc::new(move || {
            let busy = zoomit::recording().is_some();
            whole.set_visible(!busy);
            region.set_visible(!busy);
            stop.set_visible(busy);
        })
    };
    for (b, is_region) in [(&whole, false), (&region, true), (&stop, false)] {
        let (window, status, sync) = (ctx.window.clone(), status.clone(), sync.clone());
        b.connect_clicked(move |_| {
            let starting = zoomit::recording().is_none();
            let finish = {
                let (status, sync) = (status.clone(), sync.clone());
                move |result: Result<String>| {
                    match result {
                        Ok(msg) => status.set_label(&msg),
                        Err(err) => status.set_label(&format!("{err:#}")),
                    }
                    sync();
                }
            };
            // Starting a region recording needs the screen (slurp), so hide this window.
            if starting && is_region {
                run_hidden(&window, move || zoomit::record(true), finish);
            } else {
                finish(zoomit::record(false));
            }
        });
    }
    page.append(&status);
    {
        let (sync, show_level) = (sync.clone(), show_level.clone());
        page.connect_map(move |_| {
            sync();
            show_level(zoomit::zoom_factor());
        });
    }
    sync();
    page
}
