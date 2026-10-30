use crate::{calc, util, zones};
use anyhow::Result;
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeSet, HashSet};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::rc::Rc;

const MAX_RESULTS: usize = 50;

const CSS: &str = r#"
window { background: transparent; }
.panel {
    background: alpha(@window_bg_color, 0.95);
    border: 1px solid alpha(@borders, 0.8);
    border-radius: 14px;
    padding: 10px;
}
.panel entry { font-size: 18px; min-height: 44px; }
.panel scrolledwindow { margin-top: 8px; }
.panel list { background: transparent; }
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

/// One of quiver's own tools, e.g. the colour picker, run as `quiver <args>`.
#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    keywords: String,
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
    let tool = |name: &str, description: &str, keywords: &str, args: &[&str]| Tool {
        name: name.into(),
        description: description.into(),
        keywords: keywords.into(),
        args: args.iter().map(|a| a.to_string()).collect(),
    };
    let mut tools = vec![
        tool("Colour Picker", "Pick a colour from the screen", "color colour picker eyedropper hex", &["pick"]),
        tool("Text Extractor", "Copy the text from a screen region (OCR)", "ocr text extractor screenshot", &["ocr"]),
        tool("Always On Top", "Pin or unpin the focused window", "always on top pin float sticky", &["top"]),
    ];
    for layout in zones::layout_names() {
        tool_zone(&mut tools, &layout);
    }
    tools
}

fn tool_zone(tools: &mut Vec<Tool>, layout: &str) {
    tools.push(Tool {
        name: format!("Zones: {layout}"),
        description: "Tile the windows on this workspace".into(),
        keywords: format!("zones layout tile fancyzones {layout}"),
        args: vec!["zones".into(), "apply".into(), layout.into()],
    });
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

fn search(apps: &[App], tools: &[Tool], commands: impl FnOnce() -> Vec<String>, query: &str) -> Vec<Item> {
    let query = query.trim();
    if let Some(cmd) = query.strip_prefix('>') {
        return search_commands(&commands(), cmd);
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
    let mut matches: Vec<(i32, Item)> = apps
        .iter()
        .filter_map(|app| score(&app.name, query).map(|s| (s, Item::App(app.clone()))))
        .collect();
    for tool in tools {
        // Keyword hits rank a little below name hits, so "pick" finds Colour Picker
        // but an app actually called "Picker" still wins.
        let by_name = score(&tool.name, query);
        let by_keyword = tool.keywords.split_whitespace().filter_map(|k| score(k, query)).max().map(|s| s - 50);
        if let Some(s) = by_name.max(by_keyword) {
            matches.push((s, Item::Tool(tool.clone())));
        }
    }
    matches.sort_by_key(|(s, _)| -s);
    items.extend(matches.into_iter().map(|(_, item)| item));
    items.truncate(MAX_RESULTS);
    items
}

fn row(item: &Item) -> gtk::ListBoxRow {
    let (title, subtitle) = item.labels();
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let title = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["title"])
        .build();
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
        .placeholder_text("Search apps & tools,  =2*21 maths,  >cmd run")
        .build();
    let list = gtk::ListBox::new();
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(440)
        .visible(false)
        .build();
    panel.append(&entry);
    panel.append(&scroller);
    window.set_child(Some(&panel));

    let apps = Rc::new(load_apps());
    let tools = Rc::new(tools());
    // Scanning $PATH only happens the first time you type ">".
    let commands: Rc<OnceCell<Vec<String>>> = Rc::default();
    let results: Rc<RefCell<Vec<Item>>> = Rc::default();

    {
        let (list, scroller, results) = (list.clone(), scroller.clone(), results.clone());
        entry.connect_changed(move |e| {
            let cached = || commands.get_or_init(load_commands).clone();
            let items = search(&apps, &tools, cached, &e.text());
            list.remove_all();
            for item in &items {
                list.append(&row(item));
            }
            scroller.vadjustment().set_value(0.0);
            select(&list, &scroller, 0);
            scroller.set_visible(!items.is_empty());
            *results.borrow_mut() = items;
        });
    }

    let launch = {
        let (results, app) = (results.clone(), app.clone());
        Rc::new(move |index: i32, in_terminal: bool| {
            if let Some(item) = results.borrow().get(index as usize) {
                if let Err(err) = item.activate(in_terminal) {
                    util::notify("quiver", &format!("{err:#}"));
                }
            }
            app.quit();
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
        let (list, scroller, app) = (list.clone(), scroller.clone(), app.clone());
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let step = match key {
                gdk::Key::Escape => {
                    app.quit();
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

    fn apps(names: &[&str]) -> Vec<App> {
        names
            .iter()
            .map(|n| App { name: n.to_string(), exec: String::new(), comment: String::new(), terminal: false })
            .collect()
    }

    fn titles(items: &[Item]) -> Vec<String> {
        items.iter().map(|i| i.labels().0).collect()
    }

    fn no_commands() -> Vec<String> {
        panic!("PATH should only be scanned for > queries")
    }

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
        let apps = apps(&["Visual Studio Code", "Firefox", "Fire Starter"]);
        assert_eq!(titles(&search(&apps, &[], no_commands, "fire")), ["Firefox", "Fire Starter"]);
        assert_eq!(titles(&search(&apps, &[], no_commands, "vsc"))[0], "Visual Studio Code");
        assert!(matches!(search(&apps, &[], no_commands, "2*21").first(), Some(Item::Calc(r)) if r == "42"));
    }

    #[test]
    fn finds_tools_by_keyword() {
        let tools = tools();
        assert_eq!(titles(&search(&[], &tools, no_commands, "color"))[0], "Colour Picker");
        assert_eq!(titles(&search(&[], &tools, no_commands, "ocr"))[0], "Text Extractor");
        assert_eq!(titles(&search(&[], &tools, no_commands, "pin"))[0], "Always On Top");
    }

    #[test]
    fn angle_bracket_shows_only_commands() {
        let apps = apps(&["Firefox"]);
        let cmds = || vec!["firefox".to_string(), "htop".to_string(), "ls".to_string(), "lsblk".to_string()];
        let items = search(&apps, &tools(), cmds, ">");
        assert!(items.iter().all(|i| matches!(i, Item::Command(_))));
        assert_eq!(items.len(), 4);
        assert_eq!(titles(&search(&apps, &[], cmds, ">ls -la")), ["ls -la", "lsblk -la"]);
        // Unknown program: offered as typed, first.
        assert_eq!(titles(&search(&apps, &[], cmds, ">cd /tmp"))[0], "cd /tmp");
    }
}
