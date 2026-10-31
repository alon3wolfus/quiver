use super::*;

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = page(ctx, "ruler", &["ruler"]);

    let start = button("Start measuring", Some("tool-measure"), "accent");
    let intro = card();
    intro.add_css_class("flush");
    intro.set_spacing(0);
    intro.append(&setting_row(
        "tool-measure",
        "Measure the screen",
        "Freezes the screen. This window hides while you measure.",
        Some(start.upcast_ref()),
    ));
    page.append(&intro);

    page.append(&label("How it works", &["section-title"]));
    let help = card();
    help.add_css_class("flush");
    help.set_spacing(0);
    for (icon_name, title, subtitle) in [
        ("view-reveal-symbolic", "Move the mouse", "Lines reach out to the nearest colour edge in each direction, with the width × height in pixels"),
        ("zoom-fit-best-symbolic", "Drag", "Measures any box you draw"),
        ("edit-copy-symbolic", "Click", "Copies the measurement, e.g. 320x48"),
        ("process-stop-symbolic", "Esc", "Closes the ruler"),
    ] {
        help.append(&setting_row(icon_name, title, subtitle, None));
    }
    page.append(&help);
    let status = label("", &["row-subtitle"]);
    page.append(&status);

    let window = ctx.window.clone();
    start.connect_clicked(move |_| {
        let status = status.clone();
        run_quiver_hidden(&window, &["ruler"], move |result| {
            if let Err(err) = result {
                status.set_label(&format!("{err:#}"));
            }
        });
    });
    page
}
