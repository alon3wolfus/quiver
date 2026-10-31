use super::*;
use crate::rename::{self, Options};
use std::collections::HashMap;

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = page(ctx, "rename", &[]);

    let columns = hbox(16);
    columns.set_vexpand(true);

    // Left: search/replace and options, like PowerRename's left panel.
    let left = card();
    left.set_size_request(340, -1);
    left.set_valign(gtk::Align::Start);
    let find = gtk::Entry::builder().placeholder_text("Search for").css_classes(["field"]).build();
    let regex = gtk::CheckButton::with_label("Use regular expressions");
    let case = gtk::CheckButton::with_label("Case sensitive");
    let replace = gtk::Entry::builder().placeholder_text("Replace with").css_classes(["field"]).build();
    left.append(&find);
    left.append(&regex);
    left.append(&case);
    left.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    left.append(&replace);
    left.append(&label("{n} inserts a counter (1, 2, 3...). With regular expressions, $1 inserts the first group.", &["row-subtitle"]));
    let file_buttons = hbox(8);
    let add = button("Add files", Some("document-open-symbolic"), "soft");
    let clear = button("Clear", None, "soft");
    file_buttons.append(&add);
    file_buttons.append(&clear);
    left.append(&file_buttons);
    let status = label("", &["row-subtitle"]);
    left.append(&status);
    let apply = button("Apply", Some("object-select-symbolic"), "accent");
    apply.set_halign(gtk::Align::End);
    apply.set_sensitive(false);
    left.append(&apply);
    columns.append(&left);

    // Right: original and renamed names side by side.
    let right = card();
    right.add_css_class("flush");
    right.set_spacing(0);
    right.set_hexpand(true);
    let head = gtk::Box::builder().homogeneous(true).css_classes(["table-head"]).build();
    let original_head = label("Original (0)", &[]);
    let renamed_head = label("Renamed (0)", &[]);
    head.append(&original_head);
    head.append(&renamed_head);
    right.append(&head);
    let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).build();
    let empty = label("Add files to see a preview of their new names.", &["dim"]);
    empty.set_margin_top(24);
    empty.set_xalign(0.5);
    right.append(&empty);
    let scroller = gtk::ScrolledWindow::builder().child(&list).min_content_height(420).vexpand(true).build();
    right.append(&scroller);
    columns.append(&right);
    page.append(&columns);

    let files: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    let pending: Rc<RefCell<Vec<(PathBuf, PathBuf)>>> = Rc::default();

    let refresh = {
        let (find, replace, regex, case, files, pending) =
            (find.clone(), replace.clone(), regex.clone(), case.clone(), files.clone(), pending.clone());
        let (list, original_head, renamed_head, status, apply, empty) =
            (list.clone(), original_head.clone(), renamed_head.clone(), status.clone(), apply.clone(), empty.clone());
        Rc::new(move || {
            let opts = Options {
                find: find.text().to_string(),
                replace: replace.text().to_string(),
                files: files.borrow().clone(),
                regex: regex.is_active(),
                ignore_case: !case.is_active(),
                yes: false,
            };
            let mut problem = None;
            let renames = if opts.find.is_empty() {
                Vec::new()
            } else {
                match rename::plan(&opts) {
                    Ok(r) => {
                        if let Err(err) = rename::check(&r) {
                            problem = Some(format!("{err:#}"));
                        }
                        r
                    }
                    Err(err) => {
                        problem = Some(format!("{err:#}"));
                        Vec::new()
                    }
                }
            };
            let new_names: HashMap<&PathBuf, &PathBuf> = renames.iter().map(|(from, to)| (from, to)).collect();
            list.remove_all();
            for file in files.borrow().iter() {
                let row = gtk::Box::builder().homogeneous(true).build();
                let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let original = label(&name(file), &[]);
                original.set_tooltip_text(Some(&file.to_string_lossy()));
                row.append(&original);
                let renamed = label(&new_names.get(file).map(|p| name(p)).unwrap_or_default(), &["renamed"]);
                row.append(&renamed);
                list.append(&row);
            }
            empty.set_visible(files.borrow().is_empty());
            original_head.set_label(&format!("Original ({})", files.borrow().len()));
            renamed_head.set_label(&format!("Renamed ({})", renames.len()));
            status.set_label(problem.as_deref().unwrap_or(""));
            status.set_css_classes(if problem.is_some() { &["row-subtitle", "error"] } else { &["row-subtitle"] });
            apply.set_sensitive(problem.is_none() && !renames.is_empty());
            *pending.borrow_mut() = renames;
        })
    };

    for entry in [&find, &replace] {
        let refresh = refresh.clone();
        entry.connect_changed(move |_| refresh());
    }
    for check in [&regex, &case] {
        let refresh = refresh.clone();
        check.connect_toggled(move |_| refresh());
    }
    {
        let (window, files, refresh) = (ctx.window.clone(), files.clone(), refresh.clone());
        add.connect_clicked(move |_| {
            let (files, refresh) = (files.clone(), refresh.clone());
            choose_files(&window, "Choose files to rename", false, move |paths| {
                let mut files = files.borrow_mut();
                for path in paths {
                    if !files.contains(&path) {
                        files.push(path);
                    }
                }
                drop(files);
                refresh();
            });
        });
    }
    {
        let (files, refresh) = (files.clone(), refresh.clone());
        clear.connect_clicked(move |_| {
            files.borrow_mut().clear();
            refresh();
        });
    }
    {
        let (files, pending, status, refresh) = (files.clone(), pending.clone(), status.clone(), refresh.clone());
        apply.connect_clicked(move |_| {
            let renames = pending.borrow().clone();
            match rename::apply(&renames) {
                Ok(()) => {
                    // Keep showing the files under their new names.
                    for file in files.borrow_mut().iter_mut() {
                        if let Some((_, to)) = renames.iter().find(|(from, _)| from == file) {
                            *file = to.clone();
                        }
                    }
                    refresh();
                    status.set_label(&format!("Renamed {} file(s).", renames.len()));
                }
                Err(err) => {
                    refresh();
                    status.set_label(&format!("{err:#}"));
                }
            }
        });
    }
    refresh();
    page
}
