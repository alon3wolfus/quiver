use super::*;
use crate::util;

/// "Hyprland 0.56.2", or None when Hyprland isn't reachable.
fn hyprland_version() -> Option<String> {
    let version = util::hypr_query("version").ok()?;
    let tag = version["tag"].as_str().or(version["version"].as_str())?;
    Some(format!("Hyprland {}", tag.trim_start_matches('v')))
}

fn status(icon_name: &str, class: &str, title: &str, subtitle: &str) -> gtk::Box {
    let row = hbox(10);
    let image = icon(icon_name, 20);
    image.add_css_class(class);
    row.append(&image);
    let text = vbox(0);
    text.append(&label(title, &["row-title"]));
    text.append(&label(subtitle, &["row-subtitle"]));
    row.append(&text);
    row
}

fn banner(ctx: &Ctx) -> gtk::Box {
    let banner = card();
    banner.set_orientation(gtk::Orientation::Horizontal);
    banner.set_spacing(16);

    let logo = gtk::Box::builder().css_classes(["banner-icon"]).valign(gtk::Align::Center).build();
    logo.append(&icon("applications-utilities-symbolic", 32));
    banner.append(&logo);
    let text = vbox(2);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    text.append(&label("quiver", &["card-title"]));
    text.append(&label(&format!("v{}  ·  PowerToys-style tools for Hyprland", env!("CARGO_PKG_VERSION")), &["dim"]));
    banner.append(&text);

    let shortcuts = match ctx.binds.len() {
        0 => status("dialog-warning-symbolic", "warn", "No shortcuts", "Add quiver binds to your Hyprland config"),
        n => status("input-keyboard-symbolic", "ok", "Shortcuts", &format!("{n} quiver binds found")),
    };
    banner.append(&shortcuts);
    let hypr = match hyprland_version() {
        Some(version) => status("emblem-ok-symbolic", "ok", "Connected", &version),
        None => status("dialog-error-symbolic", "error", "Not connected", "Hyprland isn't running"),
    };
    banner.append(&hypr);
    banner
}

fn quick_access(ctx: &Rc<Ctx>) -> gtk::Box {
    let card = card();
    card.append(&label("Quick access", &["card-title"]));
    let grid = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(4)
        .min_children_per_line(4)
        .homogeneous(true)
        .row_spacing(4)
        .column_spacing(4)
        .build();
    for info in PAGES.iter().skip(1) {
        let tile = vbox(8);
        tile.append(&icon(info.icon, 28));
        tile.append(&gtk::Label::builder().label(info.title).justify(gtk::Justification::Center).wrap(true).build());
        let button = gtk::Button::builder().child(&tile).css_classes(["tile"]).build();
        let (ctx, id) = (ctx.clone(), info.id);
        button.connect_clicked(move |_| ctx.navigate(id));
        grid.insert(&button, -1);
    }
    card.append(&grid);
    card
}

fn shortcuts(ctx: &Ctx) -> gtk::Box {
    let card = card();
    card.append(&label("Shortcuts", &["card-title"]));
    let list = vbox(10);
    for bind in &ctx.binds {
        let row = hbox(12);
        let first = bind.args.split_whitespace().next().unwrap_or_default();
        let page = match first {
            "run" => "launcher",
            "pick" => "picker",
            "settings" => "home",
            other => other,
        };
        let icon_name = PAGES.iter().find(|p| p.id == page).map_or("input-keyboard-symbolic", |p| p.icon);
        row.append(&icon(icon_name, 16));
        let name = label(&describe(&bind.args), &[]);
        name.set_hexpand(true);
        row.append(&name);
        row.append(&keys(&bind.keys));
        list.append(&row);
    }
    if ctx.binds.is_empty() {
        list.append(&label("No quiver keybinds found in ~/.config/hypr or ~/.config/caelestia.", &["dim"]));
    }
    card.append(&list);
    card
}

fn utilities(ctx: &Rc<Ctx>) -> gtk::Box {
    let card = card();
    card.add_css_class("flush");
    card.set_spacing(0);
    let title = label("Utilities", &["card-title"]);
    title.set_margin_start(16);
    title.set_margin_top(14);
    title.set_margin_bottom(6);
    card.append(&title);
    for info in PAGES.iter().skip(1) {
        let row = hbox(14);
        row.append(&icon(info.icon, 18));
        let name = gtk::Label::builder().label(info.title).xalign(0.0).hexpand(true).build();
        row.append(&name);
        row.append(&icon("go-next-symbolic", 14));
        let button = gtk::Button::builder().child(&row).css_classes(["utility"]).build();
        let (ctx, id) = (ctx.clone(), info.id);
        button.connect_clicked(move |_| ctx.navigate(id));
        card.append(&button);
    }
    card
}

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = vbox(16);
    page.add_css_class("page");
    page.append(&label("Home", &["page-title"]));
    page.append(&banner(ctx));

    let columns = hbox(16);
    let left = vbox(16);
    left.set_hexpand(true);
    left.append(&quick_access(ctx));
    left.append(&shortcuts(ctx));
    let right = utilities(ctx);
    right.set_size_request(340, -1);
    right.set_valign(gtk::Align::Start);
    columns.append(&left);
    columns.append(&right);
    page.append(&columns);
    page
}
