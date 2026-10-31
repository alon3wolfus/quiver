use crate::{calc, util, zones};
use anyhow::Result;
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{BTreeSet, HashSet};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::rc::Rc;

const MAX_RESULTS: usize = 50;

// A Spotlight-style frosted pill with round filter buttons: light, fairly opaque blue
// glass with dark text so it reads well over any wallpaper. Hyprland blurs what's
// behind the "quiver" layer (see the README's layer_rule).
const CSS: &str = r#"
window { background: transparent; }
.glass {
    background: linear-gradient(to bottom, rgba(232, 242, 253, 0.86), rgba(204, 225, 247, 0.82));
    border: 1px solid rgba(255, 255, 255, 0.75);
    box-shadow: 0 10px 30px rgba(10, 30, 60, 0.28), inset 0 1px rgba(255, 255, 255, 0.9);
    color: #2a3646;
}
.panel.pill { border-radius: 999px; }
.panel.expanded { border-radius: 26px; padding-bottom: 8px; }
.bar { padding: 0 18px 0 20px; min-height: 56px; }
.bar image { opacity: 0.75; }
.bar entry, .bar entry:focus-within {
    background: none; border: none; outline: none; box-shadow: none;
    font-size: 21px; min-height: 56px; padding: 0;
}
.bar entry text { color: #16202c; caret-color: #0a6cff; }
.bar entry text placeholder { color: rgba(22, 32, 44, 0.5); }
.bar entry text selection { background: rgba(10, 108, 255, 0.25); color: #16202c; }
.action { color: rgba(22, 32, 44, 0.45); font-size: 17px; }
.panel separator { background: rgba(22, 32, 44, 0.12); min-height: 1px; margin: 0 16px 6px 16px; }
.filter { border-radius: 999px; min-width: 56px; min-height: 56px; padding: 0; }
/* Papirus ships these symbolic icons at a fixed 16px, so scale them when drawing. */
.filter image { -gtk-icon-transform: scale(1.4); }
.filter:hover { background: linear-gradient(to bottom, rgba(242, 248, 255, 0.95), rgba(218, 234, 252, 0.92)); }
.filter.active {
    background: linear-gradient(to bottom, rgba(64, 140, 255, 0.92), rgba(30, 110, 240, 0.9));
    border-color: rgba(255, 255, 255, 0.6);
    color: white;
}
.results { margin: 0 8px; }
.results list { background: transparent; }
.results row { border-radius: 12px; padding: 6px 10px; }
.results row:selected { background: rgba(60, 90, 130, 0.16); }
.results row:hover { background: rgba(60, 90, 130, 0.09); }
.panel label { color: #16202c; }
.title { font-weight: 600; font-size: 14px; }
.subtitle { opacity: 0.7; font-size: 12px; }
.kind { opacity: 0.5; font-size: 11px; }
.hint { opacity: 0.6; font-size: 12px; margin: 4px 12px 2px; }
"#;

/// What the round buttons next to the search bar limit results to.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Filter {
    All,
    Apps,
    Tools,
    Commands,
    Maths,
}

const FILTERS: [(Filter, &str, &str); 4] = [
    (Filter::Apps, "view-app-grid-symbolic", "Applications"),
    (Filter::Tools, "applications-utilities-symbolic", "quiver tools"),
    (Filter::Commands, "utilities-terminal-symbolic", "Commands"),
    (Filter::Maths, "accessories-calculator-symbolic", "Calculator"),
];

#[derive(Clone)]
struct App {
    name: String,
    exec: String,
    comment: String,
    icon: String,
    terminal: bool,
}

/// One of quiver's own tools, e.g. the colour picker, run as `quiver <args>`.
#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    keywords: String,
    icon: String,
    args: Vec<String>,
}

#[derive(Clone)]
enum Item {
    App(App),
    Tool(Tool),
    Calc(String),
    Command(String),
}

impl Item {
    fn labels(&self) -> (String, String) {
        match self {
            Item::App(app) => (app.name.clone(), app.comment.clone()),
            Item::Tool(tool) => (tool.name.clone(), tool.description.clone()),
            Item::Calc(result) => (format!("= {result}"), "Enter to copy".into()),
            Item::Command(cmd) => (cmd.clone(), "Enter run  ·  Shift+Enter run in terminal".into()),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Item::App(_) => "Application",
            Item::Tool(_) => "quiver",
            Item::Calc(_) => "Calculator",
            Item::Command(_) => "Command",
        }
    }

    /// What Enter does, shown after the query like Spotlight's "— Open".
    fn action(&self) -> &'static str {
        match self {
            Item::App(_) | Item::Tool(_) => "Open",
            Item::Calc(_) => "Copy",
            Item::Command(_) => "Run",
        }
    }

    fn icon(&self, size: i32) -> gtk::Image {
        let name = match self {
            Item::App(app) => app.icon.as_str(),
            Item::Tool(tool) => tool.icon.as_str(),
            Item::Calc(_) => "accessories-calculator",
            Item::Command(_) => "utilities-terminal",
        };
        // Desktop files may give a full path instead of an icon theme name.
        let image = if name.starts_with('/') {
            gtk::Image::from_file(name)
        } else if name.is_empty() {
            gtk::Image::from_icon_name("application-x-executable")
        } else {
            gtk::Image::from_icon_name(name)
        };
        image.set_pixel_size(size);
        image
    }

    fn activate(&self, in_terminal: bool) -> Result<()> {
        match self {
            Item::App(app) if app.terminal => util::spawn(&format!("{} {}", util::terminal(), app.exec)),
            Item::App(app) => util::spawn(&app.exec),
            Item::Tool(tool) => {
                let exe = std::env::current_exe()?;
                let mut cmd = util::sh_quote(&exe.to_string_lossy());
                for arg in &tool.args {
                    cmd += &format!(" {}", util::sh_quote(arg));
                }
                // Give the launcher a moment to disappear so it isn't in the picker/OCR screenshot.
                util::spawn(&format!("sleep 0.2; {cmd}"))
            }
            Item::Calc(result) => util::copy(result),
            Item::Command(cmd) if in_terminal => util::spawn_in_terminal(cmd),
            Item::Command(cmd) => util::spawn(cmd),
        }
    }
}

fn tools() -> Vec<Tool> {
    let tool = |name: &str, description: &str, keywords: &str, icon: &str, args: &[&str]| Tool {
        name: name.into(),
        description: description.into(),
        keywords: keywords.into(),
        icon: icon.into(),
        args: args.iter().map(|a| a.to_string()).collect(),
    };
    let mut tools = vec![
        tool("Colour Picker", "Pick a colour from the screen", "color colour picker eyedropper hex", "color-select-symbolic", &["pick"]),
        tool("Text Extractor", "Copy the text from a screen region (OCR)", "ocr text extractor screenshot", "insert-text-symbolic", &["ocr"]),
        tool("Always On Top", "Pin or unpin the focused window", "always on top pin float sticky", "view-pin-symbolic", &["top"]),
        tool("Quiver Settings", "Open the settings window with every tool", "settings preferences powertoys quiver rename resize", "preferences-system-symbolic", &["settings"]),
    ];
    for layout in zones::layout_names() {
        tools.push(Tool {
            name: format!("Zones: {layout}"),
            description: "Tile the windows on this workspace".into(),
            keywords: format!("zones layout tile fancyzones {layout}"),
            icon: "view-grid-symbolic".into(),
            args: vec!["zones".into(), "apply".into(), layout],
        });
    }
    tools
}

/// Directories holding .desktop files, highest priority first.
fn application_dirs() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    std::iter::once(data_home)
        .chain(data_dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from))
        .map(|d| d.join("applications"))
        .collect()
}

/// Removes desktop-entry field codes like %U or %f, which only matter when opening files.
fn clean_exec(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|arg| !(arg.len() == 2 && arg.starts_with('%') && arg != &"%%"))
        .collect::<Vec<_>>()
        .join(" ")
        .replace("%%", "%")
}

fn parse_desktop_file(text: &str) -> Option<App> {
    let mut in_entry = false;
    let (mut name, mut exec, mut comment, mut icon) = (None, None, String::new(), String::new());
    let mut terminal = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        match key.trim() {
            "Type" if value.trim() != "Application" => return None,
            "NoDisplay" | "Hidden" if value.trim() == "true" => return None,
            "Name" => name = Some(value.trim().to_string()),
            "Exec" => exec = Some(clean_exec(value)),
            "Comment" => comment = value.trim().to_string(),
            "Icon" => icon = value.trim().to_string(),
            "Terminal" => terminal = value.trim() == "true",
            _ => {}
        }
    }
    Some(App { name: name?, exec: exec?, comment, icon, terminal })
}

fn load_apps() -> Vec<App> {
    // The same file name in a higher-priority dir overrides lower ones.
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in application_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "desktop") || !seen.insert(entry.file_name()) {
                continue;
            }
            if let Some(app) = std::fs::read_to_string(&path).ok().and_then(|t| parse_desktop_file(&t)) {
                apps.push(app);
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// Every executable on $PATH, sorted and without duplicates.
fn load_commands() -> Vec<String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let mut commands = BTreeSet::new();
    for dir in path.split(':').filter(|d| !d.is_empty()) {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            // metadata() follows symlinks, so linked binaries count too.
            let executable = std::fs::metadata(entry.path())
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
            if executable {
                commands.insert(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    commands.into_iter().collect()
}

/// Higher is better; None means no match. Prefix > word start > substring > in-order letters.
fn score(name: &str, query: &str) -> Option<i32> {
    let name = name.to_lowercase();
    let query = query.to_lowercase();
    if name.starts_with(&query) {
        return Some(400 - name.len() as i32);
    }
    if name.split_whitespace().any(|w| w.starts_with(&query)) {
        return Some(300 - name.len() as i32);
    }
    if name.contains(&query) {
        return Some(200 - name.len() as i32);
    }
    let mut chars = name.chars();
    query
        .chars()
        .all(|q| chars.any(|c| c == q))
        .then(|| 100 - name.len() as i32)
}

/// ">ls -la": PATH commands matching "ls", each run with " -la" appended.
fn search_commands(commands: &[String], query: &str) -> Vec<Item> {
    let query = query.trim_start();
    let (word, args) = query.split_at(query.find(char::is_whitespace).unwrap_or(query.len()));
    if word.is_empty() {
        return commands.iter().take(MAX_RESULTS).map(|c| Item::Command(c.clone())).collect();
    }
    let mut matches: Vec<(i32, &String)> = commands
        .iter()
        .filter_map(|c| score(c, word).map(|s| (s, c)))
        .collect();
    matches.sort_by_key(|(s, _)| -s);
    let mut items: Vec<Item> = Vec::new();
    // Not a PATH program (e.g. a shell builtin or a typo)? Still offer to run it as typed.
    if !commands.iter().any(|c| c == word) {
        items.push(Item::Command(query.trim_end().to_string()));
    }
    items.extend(matches.into_iter().map(|(_, c)| Item::Command(format!("{c}{args}"))));
    items.truncate(MAX_RESULTS);
    items
}

fn search(
    apps: &[App],
    tools: &[Tool],
    commands: impl FnOnce() -> Vec<String>,
    query: &str,
    filter: Filter,
) -> Vec<Item> {
    let query = query.trim();
    if let Some(cmd) = query.strip_prefix('>') {
        return search_commands(&commands(), cmd);
    }
    if filter == Filter::Commands {
        return search_commands(&commands(), query);
    }
    let mut items = Vec::new();
    // "=2+2" always tries maths; plain "2+2" does too if it parses.
    let maths = query.strip_prefix('=').unwrap_or(query);
    let wants_maths = matches!(filter, Filter::All | Filter::Maths);
    if let Some(value) = calc::eval(maths).filter(|_| wants_maths && maths.chars().any(|c| !c.is_ascii_digit())) {
        items.push(Item::Calc(calc::format(value)));
    }
    if query.starts_with('=') || filter == Filter::Maths {
        return items;
    }
    // With a filter picked, an empty search lists everything of that kind.
    if query.is_empty() {
        match filter {
            Filter::Apps => items.extend(apps.iter().map(|a| Item::App(a.clone()))),
            Filter::Tools => items.extend(tools.iter().map(|t| Item::Tool(t.clone()))),
            _ => {}
        }
        items.truncate(MAX_RESULTS);
        return items;
    }
    let mut matches: Vec<(i32, Item)> = Vec::new();
    if matches!(filter, Filter::All | Filter::Apps) {
        matches.extend(apps.iter().filter_map(|app| score(&app.name, query).map(|s| (s, Item::App(app.clone())))));
    }
    if matches!(filter, Filter::All | Filter::Tools) {
        for tool in tools {
            // Keyword hits rank a little below name hits, so "pick" finds Colour Picker
            // but an app actually called "Picker" still wins.
            let by_name = score(&tool.name, query);
            let by_keyword = tool.keywords.split_whitespace().filter_map(|k| score(k, query)).max().map(|s| s - 50);
            if let Some(s) = by_name.max(by_keyword) {
                matches.push((s, Item::Tool(tool.clone())));
            }
        }
    }
    matches.sort_by_key(|(s, _)| -s);
    items.extend(matches.into_iter().map(|(_, item)| item));
    items.truncate(MAX_RESULTS);
    items
}

fn row(item: &Item) -> gtk::ListBoxRow {
    let (title, subtitle) = item.labels();
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hbox.append(&item.icon(32));
    let vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(1)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    let label = |text: String, class: &str| {
        gtk::Label::builder()
            .label(text)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes([class])
            .build()
    };
    vbox.append(&label(title, "title"));
    if !subtitle.is_empty() {
        vbox.append(&label(subtitle, "subtitle"));
    }
    hbox.append(&vbox);
    hbox.append(&label(item.kind().into(), "kind"));
    gtk::ListBoxRow::builder().child(&hbox).build()
}

/// Selects a row and scrolls just enough to keep it visible.
fn select(list: &gtk::ListBox, scroller: &gtk::ScrolledWindow, index: i32) {
    let Some(row) = list.row_at_index(index) else { return };
    list.select_row(Some(&row));
    let Some(bounds) = row.compute_bounds(list) else { return };
    let adj = scroller.vadjustment();
    let (top, bottom) = (bounds.y() as f64, (bounds.y() + bounds.height()) as f64);
    if top < adj.value() {
        adj.set_value(top);
    } else if bottom > adj.value() + adj.page_size() {
        adj.set_value(bottom - adj.page_size());
    }
}

/// How the launcher window lives: once (closes after use) or resident (hides after
/// use and waits on a socket to be shown again, which makes opening it instant).
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Once,
    Resident { show: bool },
}

/// Where a resident launcher listens for `quiver run`.
fn socket_path() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(runtime).join("quiver/launcher.sock")
}

fn build_ui(app: &gtk::Application, mode: Mode) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("no display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = gtk::ApplicationWindow::builder().application(app).default_width(760).build();
    // QUIVER_SNAPSHOT=file.png renders the launcher to a PNG and quits (for checking the look);
    // it then sits behind windows and leaves the keyboard alone.
    let snapshot = std::env::var_os("QUIVER_SNAPSHOT").map(PathBuf::from);

    // Layer shell turns the window into an overlay that floats above everything.
    window.init_layer_shell();
    window.set_layer(if snapshot.is_some() { Layer::Background } else { Layer::Overlay });
    window.set_keyboard_mode(if snapshot.is_some() { KeyboardMode::None } else { KeyboardMode::Exclusive });
    window.set_namespace(Some("quiver"));
    window.set_anchor(Edge::Top, true);
    window.set_margin(Edge::Top, 220);

    // Padding leaves room for the drop shadows.
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_start(24)
        .margin_end(24)
        .margin_top(24)
        .margin_bottom(32)
        .build();

    // One glass panel holds the search bar and, once you type, the results below it.
    // Empty, it's a pill with the round filter buttons beside it.
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    top.set_valign(gtk::Align::Start);
    let panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .valign(gtk::Align::Start)
        .css_classes(["glass", "panel", "pill"])
        .build();
    let bar = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(12).css_classes(["bar"]).build();
    let search_icon = gtk::Image::from_icon_name("system-search-symbolic");
    search_icon.set_pixel_size(20);
    let entry = gtk::Entry::builder().placeholder_text("Quiver Search").hexpand(true).build();
    // "— Open" and the top hit's icon at the end of the bar.
    let action = gtk::Label::builder().css_classes(["action"]).visible(false).build();
    let hit = gtk::Box::builder().valign(gtk::Align::Center).build();
    bar.append(&search_icon);
    bar.append(&entry);
    bar.append(&action);
    bar.append(&hit);
    panel.append(&bar);

    let divider = gtk::Separator::builder().visible(false).build();
    let hint = gtk::Label::builder().xalign(0.0).css_classes(["hint"]).visible(false).build();
    let list = gtk::ListBox::new();
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(460)
        .visible(false)
        .css_classes(["results"])
        .build();
    panel.append(&divider);
    panel.append(&hint);
    panel.append(&scroller);
    top.append(&panel);

    let filter = Rc::new(Cell::new(Filter::All));
    let buttons: Vec<(Filter, gtk::Button)> = FILTERS
        .iter()
        .map(|(f, icon, tooltip)| {
            let image = gtk::Image::from_icon_name(icon);
            image.set_pixel_size(22);
            let button = gtk::Button::builder()
                .child(&image)
                .tooltip_text(*tooltip)
                .css_classes(["glass", "filter"])
                .valign(gtk::Align::Start)
                .build();
            top.append(&button);
            (*f, button)
        })
        .collect();

    root.append(&top);
    window.set_child(Some(&root));

    let apps = Rc::new(RefCell::new(load_apps()));
    let tool_list = Rc::new(RefCell::new(tools()));
    // Scanning $PATH only happens the first time commands are searched.
    let commands: Rc<OnceCell<Vec<String>>> = Rc::default();
    let items: Rc<RefCell<Vec<Item>>> = Rc::default();

    // Whether the user has the launcher open (not the same as visible: the warm-up
    // frame is visible but transparent).
    let open = Rc::new(Cell::new(false));

    // Closing quits a one-off launcher but only hides a resident one.
    let close: Rc<dyn Fn()> = match mode {
        Mode::Once => {
            let app = app.clone();
            Rc::new(move || app.quit())
        }
        Mode::Resident { .. } => {
            let (window, open) = (window.clone(), open.clone());
            Rc::new(move || {
                open.set(false);
                window.set_visible(false);
            })
        }
    };

    let refresh = {
        let (list, scroller, panel, divider, buttons, items, filter, hint, entry, apps, tool_list) = (
            list.clone(),
            scroller.clone(),
            panel.clone(),
            divider.clone(),
            buttons.clone(),
            items.clone(),
            filter.clone(),
            hint.clone(),
            entry.clone(),
            apps.clone(),
            tool_list.clone(),
        );
        Rc::new(move || {
            let cached = || commands.get_or_init(load_commands).clone();
            let found = search(&apps.borrow(), &tool_list.borrow(), cached, &entry.text(), filter.get());
            // Store results before selecting a row, so the bar's top-hit icon matches.
            *items.borrow_mut() = found.clone();
            list.remove_all();
            for item in &found {
                list.append(&row(item));
            }
            scroller.vadjustment().set_value(0.0);
            select(&list, &scroller, 0);
            let active = FILTERS.iter().find(|(f, ..)| *f == filter.get());
            if let Some((_, _, name)) = active {
                hint.set_label(&format!("{name} only"));
            }
            hint.set_visible(active.is_some());
            let expanded = !found.is_empty() || active.is_some();
            scroller.set_visible(!found.is_empty());
            divider.set_visible(expanded);
            if expanded {
                panel.remove_css_class("pill");
                panel.add_css_class("expanded");
            } else {
                panel.remove_css_class("expanded");
                panel.add_css_class("pill");
            }
            // Like Spotlight, the round buttons step aside once you start typing.
            let typing = !entry.text().is_empty();
            for (_, b) in &buttons {
                b.set_visible(!typing);
            }
        })
    };
    {
        let refresh = refresh.clone();
        entry.connect_changed(move |_| refresh());
    }
    {
        let (items, action, hit) = (items.clone(), action.clone(), hit.clone());
        list.connect_row_selected(move |_, row| {
            while let Some(child) = hit.first_child() {
                hit.remove(&child);
            }
            let items = items.borrow();
            let item = row.and_then(|r| items.get(r.index() as usize));
            if let Some(item) = item {
                action.set_label(&format!("— {}", item.action()));
                hit.append(&item.icon(28));
            }
            action.set_visible(item.is_some());
        });
    }
    // Clicking a filter turns it on; clicking it again turns it off.
    for (f, button) in &buttons {
        let (f, filter, refresh, entry) = (*f, filter.clone(), refresh.clone(), entry.clone());
        let all: Vec<(Filter, gtk::Button)> = buttons.clone();
        button.connect_clicked(move |_| {
            filter.set(if filter.get() == f { Filter::All } else { f });
            for (other, b) in &all {
                if *other == filter.get() {
                    b.add_css_class("active");
                } else {
                    b.remove_css_class("active");
                }
            }
            refresh();
            entry.grab_focus();
        });
    }

    let launch = {
        let (items, close) = (items.clone(), close.clone());
        Rc::new(move |index: i32, in_terminal: bool| {
            if let Some(item) = items.borrow().get(index as usize) {
                if let Err(err) = item.activate(in_terminal) {
                    util::notify("quiver", &format!("{err:#}"));
                }
            }
            close();
        })
    };
    {
        let (list, launch) = (list.clone(), launch.clone());
        entry.connect_activate(move |_| {
            if let Some(row) = list.selected_row() {
                launch(row.index(), false);
            }
        });
    }
    {
        let launch = launch.clone();
        list.connect_row_activated(move |_, row| launch(row.index(), false));
    }

    // Arrow keys move the selection while typing stays in the entry; Escape closes.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let (list, scroller, close) = (list.clone(), scroller.clone(), close.clone());
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let step = match key {
                gdk::Key::Escape => {
                    close();
                    return glib::Propagation::Stop;
                }
                gdk::Key::Return | gdk::Key::KP_Enter if modifiers.contains(gdk::ModifierType::SHIFT_MASK) => {
                    if let Some(row) = list.selected_row() {
                        launch(row.index(), true);
                    }
                    return glib::Propagation::Stop;
                }
                gdk::Key::Down | gdk::Key::Tab => 1,
                gdk::Key::Up | gdk::Key::ISO_Left_Tab => -1,
                _ => return glib::Propagation::Proceed,
            };
            let count = list.observe_children().n_items() as i32;
            if count > 0 {
                let current = list.selected_row().map_or(0, |r| r.index());
                select(&list, &scroller, (current + step).rem_euclid(count));
            }
            glib::Propagation::Stop
        });
    }
    window.add_controller(keys);

    if mode == Mode::Once {
        if let Ok(query) = std::env::var("QUIVER_QUERY") {
            entry.set_text(&query);
        }
        window.present();
        entry.grab_focus();
        if let Some(path) = snapshot {
            let (window, app) = (window.clone(), app.clone());
            glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
                crate::gui::save_png(&window, &path);
                app.quit();
            });
        }
        return;
    }

    // Resident: start fresh each time it's shown, then pick up newly installed apps
    // and edited layouts in the background.
    let show = {
        let (window, entry, filter, buttons, refresh, open) =
            (window.clone(), entry.clone(), filter.clone(), buttons.clone(), refresh.clone(), open.clone());
        Rc::new(move || {
            open.set(true);
            // In case it's opened during the warm-up below.
            window.set_opacity(1.0);
            window.set_keyboard_mode(KeyboardMode::Exclusive);
            filter.set(Filter::All);
            for (_, b) in &buttons {
                b.remove_css_class("active");
            }
            entry.set_text("");
            refresh();
            window.present();
            entry.grab_focus();
            let (apps, tool_list) = (apps.clone(), tool_list.clone());
            glib::idle_add_local_once(move || {
                *apps.borrow_mut() = load_apps();
                *tool_list.borrow_mut() = tools();
            });
        })
    };

    let path = socket_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    // Only reached when nothing answered on the socket, so any file there is stale.
    let _ = std::fs::remove_file(&path);
    let service = gtk::gio::SocketService::new();
    let address = gtk::gio::UnixSocketAddress::new(&path);
    if let Err(err) = service.add_address(
        &address,
        gtk::gio::SocketType::Stream,
        gtk::gio::SocketProtocol::Default,
        None::<&glib::Object>,
    ) {
        eprintln!("quiver: can't listen on {}: {err}", path.display());
    }
    {
        let (open, show, close) = (open.clone(), show.clone(), close.clone());
        service.connect_incoming(move |_, connection, _| {
            // `quiver run` sends "toggle"; `quiver run --daemon` only checks we're alive.
            let mut buf = [0u8; 16];
            let n = connection.input_stream().read(&mut buf, gtk::gio::Cancellable::NONE).unwrap_or(0);
            if buf[..n].starts_with(b"toggle") {
                if open.get() {
                    close();
                } else {
                    show();
                }
            }
            false
        });
    }
    service.start();
    // Keep the socket service and the app alive for the whole session.
    std::mem::forget(service);
    std::mem::forget(app.hold());

    if mode == (Mode::Resident { show: true }) {
        show();
    } else {
        // Warm up: draw one invisible frame so icons, CSS and the renderer are loaded
        // before the first real open, then hide again.
        window.set_opacity(0.0);
        window.set_keyboard_mode(KeyboardMode::None);
        window.present();
        let window = window.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
            // Opened in the meantime? Then it's already set up for real use.
            if !open.get() {
                window.set_visible(false);
                window.set_opacity(1.0);
                window.set_keyboard_mode(KeyboardMode::Exclusive);
            }
        });
    }
}

/// `quiver run`: shows the resident launcher (starting it if needed).
/// `quiver run --daemon`: starts it hidden, e.g. at login, so the first open is instant too.
pub fn run(daemon: bool) -> Result<()> {
    let snapshot = std::env::var_os("QUIVER_SNAPSHOT").is_some();
    if !snapshot {
        // Fast path: a resident launcher is already running. Tell it to show and exit
        // without starting GTK at all.
        if let Ok(mut stream) = UnixStream::connect(socket_path()) {
            if !daemon {
                stream.write_all(b"toggle")?;
            }
            return Ok(());
        }
    }
    let mode = if snapshot { Mode::Once } else { Mode::Resident { show: !daemon } };
    let app = gtk::Application::builder()
        .application_id("dev.quiver.launcher")
        // The socket above decides which instance runs, not D-Bus.
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| build_ui(app, mode));
    // Pass no args so GTK doesn't try to parse quiver's own command line.
    app.run_with_args::<&str>(&[]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps(names: &[&str]) -> Vec<App> {
        names
            .iter()
            .map(|n| App {
                name: n.to_string(),
                exec: String::new(),
                comment: String::new(),
                icon: String::new(),
                terminal: false,
            })
            .collect()
    }

    fn titles(items: &[Item]) -> Vec<String> {
        items.iter().map(|i| i.labels().0).collect()
    }

    fn no_commands() -> Vec<String> {
        panic!("PATH should only be scanned for command searches")
    }

    fn cmds() -> Vec<String> {
        vec!["firefox".into(), "htop".into(), "ls".into(), "lsblk".into()]
    }

    #[test]
    fn strips_field_codes() {
        assert_eq!(clean_exec("firefox %u"), "firefox");
        assert_eq!(clean_exec("code --new-window %F"), "code --new-window");
        assert_eq!(clean_exec("printf 100%%"), "printf 100%");
    }

    #[test]
    fn parses_desktop_entry() {
        let text = "[Desktop Entry]\nType=Application\nName=Files\nName[de]=Dateien\nIcon=org.gnome.Nautilus\nExec=nautilus %U\n\n[Desktop Action new]\nName=New Window\nExec=nautilus --new";
        let app = parse_desktop_file(text).unwrap();
        assert_eq!((app.name.as_str(), app.exec.as_str()), ("Files", "nautilus"));
        assert_eq!(app.icon, "org.gnome.Nautilus");
        assert!(parse_desktop_file("[Desktop Entry]\nName=x\nExec=x\nNoDisplay=true").is_none());
    }

    #[test]
    fn ranks_prefix_first() {
        let apps = apps(&["Visual Studio Code", "Firefox", "Fire Starter"]);
        assert_eq!(titles(&search(&apps, &[], no_commands, "fire", Filter::All)), ["Firefox", "Fire Starter"]);
        assert_eq!(titles(&search(&apps, &[], no_commands, "vsc", Filter::All))[0], "Visual Studio Code");
        assert!(matches!(search(&apps, &[], no_commands, "2*21", Filter::All).first(), Some(Item::Calc(r)) if r == "42"));
    }

    #[test]
    fn finds_tools_by_keyword() {
        let tools = tools();
        assert_eq!(titles(&search(&[], &tools, no_commands, "color", Filter::All))[0], "Colour Picker");
        assert_eq!(titles(&search(&[], &tools, no_commands, "ocr", Filter::All))[0], "Text Extractor");
        assert_eq!(titles(&search(&[], &tools, no_commands, "pin", Filter::All))[0], "Always On Top");
    }

    #[test]
    fn angle_bracket_shows_only_commands() {
        let apps = apps(&["Firefox"]);
        let items = search(&apps, &tools(), cmds, ">", Filter::All);
        assert!(items.iter().all(|i| matches!(i, Item::Command(_))));
        assert_eq!(items.len(), 4);
        assert_eq!(titles(&search(&apps, &[], cmds, ">ls -la", Filter::All)), ["ls -la", "lsblk -la"]);
        // Unknown program: offered as typed, first.
        assert_eq!(titles(&search(&apps, &[], cmds, ">cd /tmp", Filter::All))[0], "cd /tmp");
    }

    #[test]
    fn filters_limit_results() {
        let apps = apps(&["Firefox", "Colour Mixer"]);
        let tools = tools();
        // "col" matches both an app and the Colour Picker tool; each filter keeps one kind.
        let only = |f| search(&apps, &tools, no_commands, "col", f);
        assert!(only(Filter::Apps).iter().all(|i| matches!(i, Item::App(_))));
        assert!(only(Filter::Tools).iter().all(|i| matches!(i, Item::Tool(_))));
        // The commands filter works without typing ">".
        assert_eq!(titles(&search(&apps, &tools, cmds, "hto", Filter::Commands))[1], "htop");
        // Maths only shows the result, never apps.
        assert_eq!(titles(&search(&apps, &tools, no_commands, "2*21", Filter::Maths)), ["= 42"]);
        // A filter with nothing typed lists everything of that kind.
        assert_eq!(search(&apps, &tools, no_commands, "", Filter::Apps).len(), 2);
    }
}
