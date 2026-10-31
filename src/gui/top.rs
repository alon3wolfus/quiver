use super::*;
use crate::{top, util};
use serde_json::Value;

/// Other windows, most recently focused first (this settings window is skipped).
fn windows() -> Vec<Value> {
    let clients = util::hypr_query("clients").ok().and_then(|c| c.as_array().cloned()).unwrap_or_default();
    let mut windows: Vec<Value> = clients
        .into_iter()
        .filter(|c| c["class"].as_str() != Some(APP_ID) && c["mapped"].as_bool() == Some(true))
        .filter(|c| !c["workspace"]["name"].as_str().unwrap_or_default().starts_with("special:"))
        .collect();
    windows.sort_by_key(|c| c["focusHistoryID"].as_i64().unwrap_or(i64::MAX));
    windows
}

fn title_of(win: &Value) -> String {
    let title = win["title"].as_str().unwrap_or_default();
    let class = win["class"].as_str().unwrap_or_default();
    if title.is_empty() { class.to_string() } else { title.to_string() }
}

fn fill(list: &gtk::Box, status: &gtk::Label) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let windows = windows();
    let pinned: Vec<&Value> = windows.iter().filter(|w| w["pinned"].as_bool() == Some(true)).collect();
    let others: Vec<&Value> = windows.iter().filter(|w| w["pinned"].as_bool() != Some(true)).take(6).collect();

    let add_rows = |heading: &str, wins: &[&Value], action: &str, empty: &str| {
        list.append(&label(heading, &["section-title"]));
        let card = card();
        card.add_css_class("flush");
        card.set_spacing(0);
        for win in wins {
            let address = win["address"].as_str().unwrap_or_default().to_string();
            let toggle = button(action, Some("view-pin-symbolic"), "soft");
            let subtitle = format!(
                "{}  ·  {}",
                win["class"].as_str().unwrap_or_default(),
                if win["floating"].as_bool() == Some(true) { "floating" } else { "tiled" }
            );
            card.append(&setting_row("focus-windows-symbolic", &title_of(win), &subtitle, Some(toggle.upcast_ref())));
            let (list, status) = (list.clone(), status.clone());
            toggle.connect_clicked(move |_| {
                if let Err(err) = top::toggle(Some(&address)) {
                    status.set_label(&format!("{err:#}"));
                }
                // Give Hyprland a moment to update before re-reading the windows.
                let (list, status) = (list.clone(), status.clone());
                glib::timeout_add_local_once(Duration::from_millis(250), move || fill(&list, &status));
            });
        }
        if wins.is_empty() {
            card.append(&setting_row("view-pin-symbolic", empty, "", None));
        }
        list.append(&card);
    };
    add_rows("Pinned windows", &pinned, "Unpin", "Nothing is pinned");
    add_rows("Recent windows", &others, "Pin", "No other windows open");
}

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = page(ctx, "top", &["top"]);
    page.append(&label(
        "Hyprland can only pin floating windows, so a tiled window floats in its exact spot while pinned and goes back into the grid when you unpin it.",
        &["row-subtitle"],
    ));
    let list = vbox(14);
    page.append(&list);
    let status = label("", &["row-subtitle", "error"]);
    page.append(&status);
    {
        let (list, status) = (list.clone(), status.clone());
        page.connect_map(move |_| fill(&list, &status));
    }
    page
}
