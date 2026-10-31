//! The settings window: a PowerToys-style home page plus a page per tool.

mod home;
mod launcher;
mod ocr;
mod picker;
mod rename;
mod resize;
mod top;
mod zones;

use anyhow::Result;
use gtk::prelude::*;
use gtk::{gdk, glib};
use regex::Regex;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// Also the window class Hyprland sees, so tools can skip this window.
pub const APP_ID: &str = "dev.quiver.settings";

pub struct PageInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub description: &'static str,
}

pub const PAGES: &[PageInfo] = &[
    PageInfo { id: "home", title: "Home", icon: "go-home-symbolic", description: "" },
    PageInfo {
        id: "launcher",
        title: "Launcher",
        icon: "system-search-symbolic",
        description: "Search apps and quiver's tools, do quick maths and run commands from anywhere.",
    },
    PageInfo {
        id: "zones",
        title: "Zones",
        icon: "view-grid-symbolic",
        description: "Arrange the windows on your workspace into layouts.",
    },
    PageInfo {
        id: "top",
        title: "Always On Top",
        icon: "view-pin-symbolic",
        description: "Pin a window above the others and take it with you to every workspace.",
    },
    PageInfo {
        id: "picker",
        title: "Colour Picker",
        icon: "color-select-symbolic",
        description: "Pick any colour on your screen and copy it as HEX, RGB, HSL, HSV or CMYK.",
    },
    PageInfo {
        id: "ocr",
        title: "Text Extractor",
        icon: "insert-text-symbolic",
        description: "Copy text from anywhere on your screen, even from images and videos.",
    },
    PageInfo {
        id: "rename",
        title: "Rename",
        icon: "edit-find-replace-symbolic",
        description: "Rename many files at once with find & replace, regular expressions and counters.",
    },
    PageInfo {
        id: "resize",
        title: "Image Resizer",
        icon: "image-x-generic-symbolic",
        description: "Shrink images to a preset size, an exact size or a percentage.",
    },
];

pub fn page_info(id: &str) -> &'static PageInfo {
    PAGES.iter().find(|p| p.id == id).expect("unknown page")
}

/// A quiver keybind found in the user's Hyprland Lua config.
#[derive(Clone, Debug, PartialEq)]
pub struct Bind {
    pub keys: String,
    pub args: String,
}

/// Finds `hl.bind("KEYS", ... quiver .. " ARGS")` and `hl.bind("KEYS", ... "/path/quiver ARGS")`.
fn parse_binds(text: &str) -> Vec<Bind> {
    let re = Regex::new(r#"hl\.bind\(\s*"([^"]+)"[^\n]*?quiver(?:\s*\.\.\s*"\s*|\s+)([^"\n]*)""#).unwrap();
    text.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .filter_map(|line| re.captures(line))
        .map(|c| Bind { keys: c[1].trim().to_string(), args: c[2].trim().to_string() })
        .collect()
}

fn lua_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && depth > 0 {
            lua_files(&path, depth - 1, out);
        } else if path.extension().is_some_and(|e| e == "lua") {
            out.push(path);
        }
    }
}

pub fn load_binds() -> Vec<Bind> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    let mut files = Vec::new();
    lua_files(&config.join("hypr"), 3, &mut files);
    lua_files(&config.join("caelestia"), 1, &mut files);
    let mut binds: Vec<Bind> = Vec::new();
    for file in files {
        for bind in parse_binds(&std::fs::read_to_string(file).unwrap_or_default()) {
            if !binds.contains(&bind) {
                binds.push(bind);
            }
        }
    }
    binds
}

/// "zones snap columns 1" → "Snap to columns, zone 1".
pub fn describe(args: &str) -> String {
    let words: Vec<&str> = args.split_whitespace().collect();
    match words.as_slice() {
        ["run"] => "Open the launcher".into(),
        ["pick", ..] => "Pick a colour".into(),
        ["ocr", ..] => "Extract text from the screen".into(),
        ["top", ..] => "Pin or unpin a window".into(),
        ["settings"] => "Open quiver settings".into(),
        ["zones", "apply", layout, ..] => format!("Tile windows: {layout}"),
        ["zones", "snap", layout, zone, ..] => format!("Snap to {layout}, zone {zone}"),
        _ => format!("quiver {args}"),
    }
}

/// Shared state the pages need: the window and a way to switch pages.
pub struct Ctx {
    pub window: gtk::ApplicationWindow,
    pub binds: Vec<Bind>,
    navigate: RefCell<Option<Box<dyn Fn(&str)>>>,
}

impl Ctx {
    pub fn navigate(&self, id: &str) {
        if let Some(go) = self.navigate.borrow().as_ref() {
            go(id);
        }
    }
}

// ---- Small widget helpers shared by the pages ----

pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    gtk::Label::builder().label(text).xalign(0.0).wrap(true).css_classes(classes.to_vec()).build()
}

pub fn vbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, spacing)
}

pub fn hbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, spacing)
}

pub fn card() -> gtk::Box {
    let card = vbox(12);
    card.add_css_class("card");
    card
}

pub fn icon(name: &str, size: i32) -> gtk::Image {
    let image = gtk::Image::from_icon_name(name);
    image.set_pixel_size(size);
    image
}

pub fn button(text: &str, icon_name: Option<&str>, class: &str) -> gtk::Button {
    let inner = hbox(8);
    if let Some(name) = icon_name {
        inner.append(&icon(name, 16));
    }
    inner.append(&gtk::Label::new(Some(text)));
    gtk::Button::builder().child(&inner).css_classes([class]).build()
}

pub fn icon_button(icon_name: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon_name)
        .tooltip_text(tooltip)
        .css_classes(["flat", "icon-only"])
        .valign(gtk::Align::Center)
        .build()
}

/// "SUPER + CTRL + T" as separate key chips.
pub fn keys(keys: &str) -> gtk::Box {
    let row = hbox(4);
    row.set_valign(gtk::Align::Center);
    for key in keys.split('+').map(str::trim).filter(|k| !k.is_empty()) {
        let text = match key.to_uppercase().as_str() {
            "SUPER" => "Super".to_string(),
            "CTRL" | "CONTROL" => "Ctrl".to_string(),
            "SHIFT" => "Shift".to_string(),
            "ALT" => "Alt".to_string(),
            "SPACE" => "Space".to_string(),
            "LEFT" => "←".to_string(),
            "RIGHT" => "→".to_string(),
            "UP" => "↑".to_string(),
            "DOWN" => "↓".to_string(),
            _ if key.len() == 1 => key.to_uppercase(),
            _ => key.to_string(),
        };
        row.append(&gtk::Label::builder().label(text).css_classes(["key"]).build());
    }
    row
}

/// A row with an icon, title, subtitle and a widget on the right, like PowerToys settings.
pub fn setting_row(icon_name: &str, title: &str, subtitle: &str, end: Option<&gtk::Widget>) -> gtk::Box {
    let row = hbox(14);
    row.add_css_class("setting-row");
    row.append(&icon(icon_name, 18));
    let text = vbox(2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    text.append(&label(title, &["row-title"]));
    if !subtitle.is_empty() {
        text.append(&label(subtitle, &["row-subtitle"]));
    }
    row.append(&text);
    if let Some(end) = end {
        end.set_valign(gtk::Align::Center);
        row.append(end);
    }
    row
}

/// The page's title, description and a "Shortcuts" card for binds starting with `args`.
pub fn page(ctx: &Ctx, id: &str, args: &[&str]) -> gtk::Box {
    let info = page_info(id);
    let page = vbox(14);
    page.add_css_class("page");
    page.append(&label(info.title, &["page-title"]));
    page.append(&label(info.description, &["page-subtitle"]));
    if !args.is_empty() {
        page.append(&label("Shortcuts", &["section-title"]));
        let card = card();
        card.add_css_class("flush");
        card.set_spacing(0);
        let matching: Vec<&Bind> = ctx
            .binds
            .iter()
            .filter(|b| args.iter().any(|a| b.args == *a || b.args.starts_with(&format!("{a} "))))
            .collect();
        for bind in &matching {
            let chips = keys(&bind.keys);
            card.append(&setting_row("input-keyboard-symbolic", &describe(&bind.args), &format!("quiver {}", bind.args), Some(chips.upcast_ref())));
        }
        if matching.is_empty() {
            let hint = format!(
                "Add one to your Hyprland config, e.g. hl.bind(\"SUPER + ALT + X\", hl.dsp.exec_cmd(\"quiver {}\"))",
                args[0]
            );
            card.append(&setting_row("input-keyboard-symbolic", "No shortcut set", &hint, None));
        }
        page.append(&card);
    }
    page
}

/// Hides the window, runs `work` on a background thread (so screen pickers don't
/// capture this window), then shows the window again and calls `done`.
pub fn run_hidden<T: Send + 'static>(
    window: &gtk::ApplicationWindow,
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(T) + 'static,
) {
    window.set_visible(false);
    let window = window.clone();
    glib::spawn_future_local(async move {
        let result = gtk::gio::spawn_blocking(move || {
            std::thread::sleep(Duration::from_millis(250));
            work()
        })
        .await;
        window.present();
        if let Ok(value) = result {
            done(value);
        }
    });
}

/// Runs slow work (like resizing images) off the UI thread.
pub fn run_background<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    glib::spawn_future_local(async move {
        if let Ok(value) = gtk::gio::spawn_blocking(work).await {
            done(value);
        }
    });
}

/// Opens a file picker for several files and passes the chosen paths to `done`.
pub fn choose_files(window: &gtk::ApplicationWindow, title: &str, images_only: bool, done: impl Fn(Vec<PathBuf>) + 'static) {
    let dialog = gtk::FileDialog::builder().title(title).modal(true).build();
    if images_only {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Images"));
        filter.add_mime_type("image/*");
        let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
    }
    dialog.open_multiple(Some(window), gtk::gio::Cancellable::NONE, move |result| {
        let Ok(files) = result else { return };
        let paths = (0..files.n_items())
            .filter_map(|i| files.item(i).and_downcast::<gtk::gio::File>())
            .filter_map(|f| f.path())
            .collect();
        done(paths);
    });
}

/// Renders the window to a PNG; used by QUIVER_SNAPSHOT to check the look without a screen.
pub fn save_png(widget: &impl IsA<gtk::Widget>, path: &Path) {
    let (w, h) = (widget.width() as f64, widget.height() as f64);
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, w, h);
    let (Some(node), Some(renderer)) = (snapshot.to_node(), widget.native().and_then(|n| n.renderer())) else {
        return;
    };
    if let Err(err) = renderer.render_texture(&node, None).save_to_png(path) {
        eprintln!("snapshot {}: {err}", path.display());
    }
}

fn sidebar_row(info: &PageInfo) -> gtk::ListBoxRow {
    let row = hbox(12);
    let indicator = gtk::Box::builder().css_classes(["indicator"]).valign(gtk::Align::Center).build();
    row.append(&indicator);
    row.append(&icon(info.icon, 16));
    row.append(&gtk::Label::new(Some(info.title)));
    gtk::ListBoxRow::builder().child(&row).name(info.id).build()
}

fn build_ui(app: &gtk::Application) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("no display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Quiver Settings")
        .default_width(1180)
        .default_height(780)
        .css_classes(["quiver"])
        .build();

    // A slim title bar like PowerToys: app icon and name on the left.
    let header = gtk::HeaderBar::new();
    let brand = hbox(8);
    brand.append(&icon("applications-utilities-symbolic", 16));
    brand.append(&gtk::Label::builder().label("Quiver Settings").css_classes(["app-name"]).build());
    header.pack_start(&brand);
    header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    window.set_titlebar(Some(&header));

    let ctx = Rc::new(Ctx { window: window.clone(), binds: load_binds(), navigate: RefCell::new(None) });

    let sidebar_list = gtk::ListBox::new();
    for (i, info) in PAGES.iter().enumerate() {
        sidebar_list.append(&sidebar_row(info));
        if i == 0 {
            // Separate Home from the tools, like PowerToys' groups.
            let heading = gtk::ListBoxRow::builder()
                .child(&label("TOOLS", &["sidebar-heading"]))
                .selectable(false)
                .activatable(false)
                .build();
            sidebar_list.append(&heading);
        }
    }
    let sidebar = vbox(0);
    sidebar.add_css_class("sidebar");
    sidebar.append(&sidebar_list);

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(120)
        .hexpand(true)
        .vexpand(true)
        .build();
    let pages: Vec<(&str, gtk::Widget)> = vec![
        ("home", home::build(&ctx).upcast()),
        ("launcher", launcher::build(&ctx).upcast()),
        ("zones", zones::build(&ctx).upcast()),
        ("top", top::build(&ctx).upcast()),
        ("picker", picker::build(&ctx).upcast()),
        ("ocr", ocr::build(&ctx).upcast()),
        ("rename", rename::build(&ctx).upcast()),
        ("resize", resize::build(&ctx).upcast()),
    ];
    for (id, page) in pages {
        let scroller = gtk::ScrolledWindow::builder()
            .child(&page)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        stack.add_named(&scroller, Some(id));
    }

    {
        let stack = stack.clone();
        sidebar_list.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                stack.set_visible_child_name(&row.widget_name());
            }
        });
    }
    {
        let list = sidebar_list.clone();
        *ctx.navigate.borrow_mut() = Some(Box::new(move |id: &str| {
            let mut i = 0;
            while let Some(row) = list.row_at_index(i) {
                if row.widget_name() == id {
                    list.select_row(Some(&row));
                    break;
                }
                i += 1;
            }
        }));
    }
    // QUIVER_PAGE opens a page other than Home (handy for screenshots).
    let start = std::env::var("QUIVER_PAGE").ok().filter(|id| PAGES.iter().any(|p| p.id == id));
    ctx.navigate(start.as_deref().unwrap_or("home"));

    let content = gtk::Box::builder().css_classes(["content"]).build();
    content.append(&stack);
    let body = hbox(0);
    body.append(&sidebar);
    body.append(&content);
    window.set_child(Some(&body));
    window.present();

    // QUIVER_SNAPSHOT=file.png renders the window to a PNG and quits, so the look can be
    // checked without showing it. Only the first frame is reliable when the window isn't
    // visible, so render one page per run.
    if let Some(path) = std::env::var_os("QUIVER_SNAPSHOT").map(PathBuf::from) {
        let (window, app) = (window.clone(), app.clone());
        glib::timeout_add_local_once(Duration::from_millis(800), move || {
            save_png(&window, &path);
            app.quit();
        });
    }
}

pub fn run() -> Result<()> {
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run_with_args::<&str>(&[]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_quiver_binds_in_lua() {
        let text = r#"
local quiver = os.getenv("HOME") .. "/.cargo/bin/quiver"
hl.bind("ALT + SPACE", hl.dsp.exec_cmd(quiver .. " run"))                  -- Launcher
hl.bind("SUPER + ALT + Left", hl.dsp.exec_cmd(quiver .. " zones snap columns 1"))
hl.bind("SUPER + P", hl.dsp.exec_cmd("/home/me/.cargo/bin/quiver top"))
-- hl.bind("SUPER + X", hl.dsp.exec_cmd(quiver .. " ocr"))
hl.bind("SUPER + T", hl.dsp.exec_cmd("foot"))
"#;
        let binds = parse_binds(text);
        assert_eq!(
            binds,
            [
                Bind { keys: "ALT + SPACE".into(), args: "run".into() },
                Bind { keys: "SUPER + ALT + Left".into(), args: "zones snap columns 1".into() },
                Bind { keys: "SUPER + P".into(), args: "top".into() },
            ]
        );
    }

    #[test]
    fn describes_binds() {
        assert_eq!(describe("zones snap columns 1"), "Snap to columns, zone 1");
        assert_eq!(describe("zones apply grid"), "Tile windows: grid");
        assert_eq!(describe("pick --format rgb"), "Pick a colour");
    }
}
