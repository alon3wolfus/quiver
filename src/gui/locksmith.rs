use super::*;
use crate::locksmith::{self, Holder};

fn fill(list: &gtk::Box, targets: &[PathBuf], status: &gtk::Label, refresh: Rc<dyn Fn()>) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if targets.is_empty() {
        list.append(&setting_row("folder-symbolic", "Add files or folders to see what's using them", "", None));
        return;
    }
    let holders: Vec<Holder> = locksmith::find(targets);
    if holders.is_empty() {
        list.append(&setting_row("emblem-ok-symbolic", "Nothing is using these files", "", None));
    }
    for h in holders {
        let end = button("End task", Some("process-stop-symbolic"), "soft");
        let subtitle = format!("PID {}  ·  {}", h.pid, h.uses.join("  ·  "));
        let row = setting_row("application-x-executable-symbolic", &h.name, &subtitle, Some(end.upcast_ref()));
        row.set_tooltip_text(Some(&h.command));
        let (status, refresh, pid, name) = (status.clone(), refresh.clone(), h.pid, h.name.clone());
        end.connect_clicked(move |_| {
            match locksmith::end(pid) {
                Ok(()) => status.set_label(&format!("Asked {name} (PID {pid}) to quit.")),
                Err(err) => status.set_label(&format!("{err:#}")),
            }
            let refresh = refresh.clone();
            glib::timeout_add_local_once(Duration::from_millis(400), move || refresh());
        });
        list.append(&row);
    }
}

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = page(ctx, "locksmith", &[]);

    let head = hbox(8);
    let title = label("Selected (0)", &["section-title"]);
    title.set_hexpand(true);
    head.append(&title);
    let add_files = button("Add files", Some("document-open-symbolic"), "soft");
    let add_folder = button("Add folder", Some("folder-symbolic"), "soft");
    let clear = button("Clear", None, "soft");
    let scan = button("Refresh", Some("view-refresh-symbolic"), "accent");
    for b in [&add_files, &add_folder, &clear, &scan] {
        head.append(b);
    }
    page.append(&head);
    let selected = label("", &["row-subtitle"]);
    page.append(&selected);

    page.append(&label("Processes using them", &["section-title"]));
    let list = card();
    list.add_css_class("flush");
    list.set_spacing(0);
    page.append(&list);
    let status = label("", &["row-subtitle"]);
    page.append(&status);
    page.append(&label(
        "Processes of other users (like system services) only show up when quiver runs as root.",
        &["row-subtitle"],
    ));

    let targets: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    let refresh: Rc<dyn Fn()> = {
        let (list, targets, status, title, selected) = (list.clone(), targets.clone(), status.clone(), title.clone(), selected.clone());
        let me: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::default();
        let f: Rc<dyn Fn()> = {
            let me = me.clone();
            Rc::new(move || {
                let t = targets.borrow().clone();
                title.set_label(&format!("Selected ({})", t.len()));
                selected.set_label(&t.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n"));
                let again = me.borrow().clone().unwrap();
                fill(&list, &t, &status, again);
            })
        };
        *me.borrow_mut() = Some(f.clone());
        f
    };

    let add = {
        let (targets, refresh) = (targets.clone(), refresh.clone());
        Rc::new(move |paths: Vec<PathBuf>| {
            let mut t = targets.borrow_mut();
            for p in paths {
                if !t.contains(&p) {
                    t.push(p);
                }
            }
            drop(t);
            refresh();
        })
    };
    {
        let (window, add) = (ctx.window.clone(), add.clone());
        add_files.connect_clicked(move |_| {
            let add = add.clone();
            choose_files(&window, "Choose files", false, move |paths| add(paths));
        });
    }
    {
        let (window, add) = (ctx.window.clone(), add.clone());
        add_folder.connect_clicked(move |_| {
            let dialog = gtk::FileDialog::builder().title("Choose a folder").modal(true).build();
            let add = add.clone();
            dialog.select_folder(Some(&window), gtk::gio::Cancellable::NONE, move |result| {
                if let Some(path) = result.ok().and_then(|f| f.path()) {
                    add(vec![path]);
                }
            });
        });
    }
    {
        let (targets, refresh) = (targets.clone(), refresh.clone());
        clear.connect_clicked(move |_| {
            targets.borrow_mut().clear();
            refresh();
        });
    }
    {
        let refresh = refresh.clone();
        scan.connect_clicked(move |_| refresh());
    }
    refresh();
    page
}
