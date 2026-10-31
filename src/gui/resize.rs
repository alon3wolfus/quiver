use super::*;
use crate::resize;

const SIZES: [(&str, &str); 5] = [
    ("Small (854 × 480)", "small"),
    ("Medium (1366 × 768)", "medium"),
    ("Large (1920 × 1080)", "large"),
    ("Phone (320 × 568)", "phone"),
    ("Custom", ""),
];

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = page(ctx, "resize", &[]);

    let settings = card();
    settings.add_css_class("flush");
    settings.set_spacing(0);
    let names: Vec<&str> = SIZES.iter().map(|(name, _)| *name).collect();
    let size = gtk::DropDown::from_strings(&names);
    size.set_selected(1);
    settings.append(&setting_row("zoom-fit-best-symbolic", "Size", "Images keep their aspect ratio and are never enlarged", Some(size.upcast_ref())));
    let custom = gtk::Entry::builder().placeholder_text("1280x720 or 50%").width_chars(14).css_classes(["field"]).build();
    let custom_row = setting_row("insert-text-symbolic", "Custom size", "WIDTHxHEIGHT to fit inside, or a percentage", Some(custom.upcast_ref()));
    custom_row.set_visible(false);
    settings.append(&custom_row);
    let overwrite = gtk::Switch::new();
    settings.append(&setting_row(
        "document-save-symbolic",
        "Replace originals",
        "Off: saves a copy next to each image, e.g. photo (Medium).jpg",
        Some(overwrite.upcast_ref()),
    ));
    page.append(&settings);

    let files_card = card();
    let head = hbox(8);
    let title = label("Images (0)", &["card-title"]);
    title.set_hexpand(true);
    head.append(&title);
    let add = button("Add images", Some("image-x-generic-symbolic"), "soft");
    let clear = button("Clear", None, "soft");
    let run = button("Resize", Some("object-select-symbolic"), "accent");
    run.set_sensitive(false);
    head.append(&add);
    head.append(&clear);
    head.append(&run);
    files_card.append(&head);
    let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).build();
    let empty = label("Add images to resize them.", &["dim"]);
    empty.set_xalign(0.5);
    empty.set_margin_top(16);
    empty.set_margin_bottom(16);
    files_card.append(&empty);
    files_card.append(&list);
    page.append(&files_card);

    let files: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    let show = {
        let (list, title, run, files, empty) = (list.clone(), title.clone(), run.clone(), files.clone(), empty.clone());
        Rc::new(move |results: Option<Vec<String>>| {
            list.remove_all();
            let files = files.borrow();
            empty.set_visible(files.is_empty());
            for (i, file) in files.iter().enumerate() {
                let text = match &results {
                    Some(lines) => lines[i].clone(),
                    None => file.display().to_string(),
                };
                let error = text.contains("can't") || text.contains("error");
                list.append(&label(&text, if error { &["error"] } else { &[] }));
            }
            title.set_label(&format!("Images ({})", files.len()));
            run.set_sensitive(!files.is_empty());
        })
    };

    {
        let custom_row = custom_row.clone();
        size.connect_selected_notify(move |dd| custom_row.set_visible(dd.selected() as usize == SIZES.len() - 1));
    }
    {
        let (window, files, show) = (ctx.window.clone(), files.clone(), show.clone());
        add.connect_clicked(move |_| {
            let (files, show) = (files.clone(), show.clone());
            choose_files(&window, "Choose images to resize", true, move |paths| {
                let mut list = files.borrow_mut();
                for path in paths {
                    if !list.contains(&path) {
                        list.push(path);
                    }
                }
                drop(list);
                show(None);
            });
        });
    }
    {
        let (files, show) = (files.clone(), show.clone());
        clear.connect_clicked(move |_| {
            files.borrow_mut().clear();
            show(None);
        });
    }
    {
        let (files, show) = (files.clone(), show.clone());
        run.connect_clicked(move |button| {
            let chosen = SIZES[size.selected() as usize].1;
            let text = if chosen.is_empty() { custom.text().to_string() } else { chosen.to_string() };
            let parsed = match resize::parse_size(&text) {
                Ok(parsed) => parsed,
                Err(err) => {
                    let lines = vec![format!("{err:#}"); files.borrow().len()];
                    show(Some(lines));
                    return;
                }
            };
            let (paths, overwrite, show, button) = (files.borrow().clone(), overwrite.is_active(), show.clone(), button.clone());
            button.set_sensitive(false);
            run_background(
                move || {
                    paths
                        .iter()
                        .map(|p| resize::resize_one(p, &parsed, overwrite).unwrap_or_else(|e| format!("{}: {e:#}", p.display())))
                        .collect::<Vec<_>>()
                },
                move |lines| {
                    show(Some(lines));
                    button.set_sensitive(true);
                },
            );
        });
    }
    show(None);
    page
}
