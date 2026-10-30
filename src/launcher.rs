use crate::{calc, util};
use anyhow::Result;
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

const MAX_RESULTS: usize = 8;

const CSS: &str = r#"
window { background: transparent; }
.panel {
    background: alpha(@window_bg_color, 0.95);
    border: 1px solid alpha(@borders, 0.8);
    border-radius: 14px;
    padding: 10px;
}
.panel entry { font-size: 18px; min-height: 44px; }
.panel list { background: transparent; margin-top: 8px; }
.panel row { border-radius: 8px; padding: 6px 10px; }
.title { font-weight: bold; }
.subtitle { opacity: 0.65; font-size: 12px; }
"#;

#[derive(Clone)]
struct App {
    name: String,
    exec: String,
    comment: String,
    terminal: bool,
}

#[derive(Clone)]
enum Item {
    App(App),
    Calc(String),
    Command(String),
}

impl Item {
    fn labels(&self) -> (String, String) {
        match self {
            Item::App(app) => (app.name.clone(), app.comment.clone()),
            Item::Calc(result) => (format!("= {result}"), "Enter to copy".into()),
            Item::Command(cmd) => (cmd.clone(), "Run command".into()),
        }
    }

    fn activate(&self) -> Result<()> {
        match self {
            Item::App(app) if app.terminal => {
                let term = std::env::var("TERMINAL").unwrap_or_else(|_| "kitty".into());
                util::spawn(&format!("{term} -e {}", app.exec))
            }
            Item::App(app) => util::spawn(&app.exec),
            Item::Calc(result) => util::copy(result),
            Item::Command(cmd) => util::spawn(cmd),
        }
    }
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
    let (mut name, mut exec, mut comment) = (None, None, String::new());
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
            "Terminal" => terminal = value.trim() == "true",
            _ => {}
        }
    }
    Some(App { name: name?, exec: exec?, comment, terminal })
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

fn search(apps: &[App], query: &str) -> Vec<Item> {
    let query = query.trim();
    if let Some(cmd) = query.strip_prefix('>') {
        let cmd = cmd.trim();
        return if cmd.is_empty() { vec![] } else { vec![Item::Command(cmd.into())] };
    }
    let mut items = Vec::new();
    // "=2+2" always tries maths; plain "2+2" does too if it parses.
    let maths = query.strip_prefix('=').unwrap_or(query);
    if let Some(value) = calc::eval(maths).filter(|_| maths.chars().any(|c| !c.is_ascii_digit())) {
        items.push(Item::Calc(calc::format(value)));
    }
    if query.is_empty() || query.starts_with('=') {
        return items;
    }
    let mut matches: Vec<(i32, &App)> = apps
        .iter()
        .filter_map(|app| score(&app.name, query).map(|s| (s, app)))
        .collect();
    matches.sort_by_key(|(s, _)| -s);
    items.extend(matches.into_iter().map(|(_, app)| Item::App(app.clone())));
    items.truncate(MAX_RESULTS);
    items
}

fn row(item: &Item) -> gtk::ListBoxRow {
    let (title, subtitle) = item.labels();
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let title = gtk::Label::builder().label(title).xalign(0.0).css_classes(["title"]).build();
    vbox.append(&title);
    if !subtitle.is_empty() {
        let sub = gtk::Label::builder()
            .label(subtitle)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["subtitle"])
            .build();
        vbox.append(&sub);
    }
    gtk::ListBoxRow::builder().child(&vbox).build()
}

fn build_ui(app: &gtk::Application) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("no display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = gtk::ApplicationWindow::builder().application(app).default_width(640).build();
    // Layer shell turns the window into an overlay that floats above everything.
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_namespace(Some("quiver"));
    window.set_anchor(Edge::Top, true);
    window.set_margin(Edge::Top, 240);

    let panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["panel"])
        .build();
    let entry = gtk::Entry::builder()
        .placeholder_text("Search apps,  =2*21  maths,  >cmd  run")
        .build();
    let list = gtk::ListBox::new();
    list.set_visible(false);
    panel.append(&entry);
    panel.append(&list);
    window.set_child(Some(&panel));

    let apps = Rc::new(load_apps());
    let results: Rc<RefCell<Vec<Item>>> = Rc::default();

    let refresh = {
        let (list, results) = (list.clone(), results.clone());
        move |query: &str| {
            list.remove_all();
            let items = search(&apps, query);
            for item in &items {
                list.append(&row(item));
            }
            list.select_row(list.row_at_index(0).as_ref());
            list.set_visible(!items.is_empty());
            *results.borrow_mut() = items;
        }
    };
    entry.connect_changed(move |e| refresh(&e.text()));

    let launch = {
        let (results, app) = (results.clone(), app.clone());
        move |index: i32| {
            if let Some(item) = results.borrow().get(index as usize) {
                if let Err(err) = item.activate() {
                    util::notify("quiver", &err.to_string());
                }
            }
            app.quit();
        }
    };
    let launch = Rc::new(launch);
    {
        let (list, launch) = (list.clone(), launch.clone());
        entry.connect_activate(move |_| {
            if let Some(row) = list.selected_row() {
                launch(row.index());
            }
        });
    }
    list.connect_row_activated(move |_, row| launch(row.index()));

    // Arrow keys move the selection while typing stays in the entry; Escape closes.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let (list, app) = (list.clone(), app.clone());
        keys.connect_key_pressed(move |_, key, _, _| {
            let step = match key {
                gdk::Key::Escape => {
                    app.quit();
                    return glib::Propagation::Stop;
                }
                gdk::Key::Down | gdk::Key::Tab => 1,
                gdk::Key::Up | gdk::Key::ISO_Left_Tab => -1,
                _ => return glib::Propagation::Proceed,
            };
            let count = list.observe_children().n_items() as i32;
            if count > 0 {
                let current = list.selected_row().map_or(0, |r| r.index());
                let next = (current + step).rem_euclid(count);
                list.select_row(list.row_at_index(next).as_ref());
            }
            glib::Propagation::Stop
        });
    }
    window.add_controller(keys);

    window.present();
    entry.grab_focus();
}

pub fn run() -> Result<()> {
    let app = gtk::Application::builder().application_id("dev.quiver.launcher").build();
    app.connect_activate(build_ui);
    // Pass no args so GTK doesn't try to parse quiver's own command line.
    app.run_with_args::<&str>(&[]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_field_codes() {
        assert_eq!(clean_exec("firefox %u"), "firefox");
        assert_eq!(clean_exec("code --new-window %F"), "code --new-window");
        assert_eq!(clean_exec("printf 100%%"), "printf 100%");
    }

    #[test]
    fn parses_desktop_entry() {
        let text = "[Desktop Entry]\nType=Application\nName=Files\nName[de]=Dateien\nExec=nautilus %U\n\n[Desktop Action new]\nName=New Window\nExec=nautilus --new";
        let app = parse_desktop_file(text).unwrap();
        assert_eq!((app.name.as_str(), app.exec.as_str()), ("Files", "nautilus"));
        assert!(parse_desktop_file("[Desktop Entry]\nName=x\nExec=x\nNoDisplay=true").is_none());
    }

    #[test]
    fn ranks_prefix_first() {
        let apps: Vec<App> = ["Visual Studio Code", "Firefox", "Fire Starter"]
            .iter()
            .map(|n| App { name: n.to_string(), exec: String::new(), comment: String::new(), terminal: false })
            .collect();
        let names: Vec<String> = search(&apps, "fire").iter().map(|i| i.labels().0).collect();
        assert_eq!(names, ["Firefox", "Fire Starter"]);
        assert!(matches!(search(&apps, "vsc").first(), Some(Item::App(a)) if a.name == "Visual Studio Code"));
        assert!(matches!(search(&apps, "2*21").first(), Some(Item::Calc(r)) if r == "42"));
    }
}
