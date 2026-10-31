use super::*;
use crate::ocr;

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = page(ctx, "ocr", &["ocr"]);
    let card = card();

    let languages = ocr::languages();
    let names: Vec<&str> = languages.iter().map(String::as_str).collect();
    let language = gtk::DropDown::from_strings(&names);
    if let Some(eng) = languages.iter().position(|l| l == "eng") {
        language.set_selected(eng as u32);
    }
    let capture = button("Select region", Some("insert-text-symbolic"), "accent");
    let settings = card_rows(&language, &capture);
    page.append(&settings);

    let head = hbox(8);
    let title = label("Extracted text", &["card-title"]);
    title.set_hexpand(true);
    head.append(&title);
    let copy = icon_button("edit-copy-symbolic", "Copy text");
    head.append(&copy);
    card.append(&head);
    let text = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .editable(true)
        .css_classes(["result"])
        .build();
    text.buffer().set_text("Select a region of the screen and its text appears here.");
    let scroller = gtk::ScrolledWindow::builder().child(&text).min_content_height(260).build();
    card.append(&scroller);
    let status = label("", &["row-subtitle"]);
    card.append(&status);
    page.append(&card);

    {
        let text = text.clone();
        copy.connect_clicked(move |b| {
            let buffer = text.buffer();
            b.clipboard().set_text(&buffer.text(&buffer.start_iter(), &buffer.end_iter(), false));
        });
    }
    {
        let (window, text, status) = (ctx.window.clone(), text.clone(), status.clone());
        capture.connect_clicked(move |_| {
            let lang = languages.get(language.selected() as usize).cloned().unwrap_or_else(|| "eng".into());
            let (text, status) = (text.clone(), status.clone());
            run_hidden(&window, move || ocr::capture(&lang), move |result| match result {
                Ok(Some(found)) if found.is_empty() => status.set_label("No text found. Try selecting a larger area."),
                Ok(Some(found)) => {
                    text.buffer().set_text(&found);
                    text.clipboard().set_text(&found);
                    status.set_label("Copied to the clipboard.");
                }
                Ok(None) => {}
                Err(err) => status.set_label(&format!("{err:#}")),
            });
        });
    }
    page
}

/// The language picker and capture button as PowerToys-style setting rows.
fn card_rows(language: &gtk::DropDown, capture: &gtk::Button) -> gtk::Box {
    let card = card();
    card.add_css_class("flush");
    card.set_spacing(0);
    card.append(&setting_row("preferences-desktop-locale-symbolic", "Language", "Which tesseract language data to read with", Some(language.upcast_ref())));
    card.append(&setting_row("zoom-fit-best-symbolic", "Capture", "Drag a box over the text you want. Escape cancels.", Some(capture.upcast_ref())));
    card
}
