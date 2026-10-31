use super::*;
use crate::util;

pub fn build(ctx: &Rc<Ctx>) -> gtk::Box {
    let page = page(ctx, "launcher", &["run"]);

    let open = button("Open launcher", Some("system-search-symbolic"), "accent");
    open.connect_clicked(|_| {
        if let Ok(exe) = std::env::current_exe() {
            let cmd = format!("{} run", util::sh_quote(&exe.to_string_lossy()));
            if let Err(err) = util::spawn(&cmd) {
                util::notify("quiver", &format!("{err:#}"));
            }
        }
    });
    let intro = card();
    intro.add_css_class("flush");
    intro.set_spacing(0);
    intro.append(&setting_row(
        "system-search-symbolic",
        "Spotlight-style search",
        "A floating search bar with app icons. The round buttons next to it limit results to apps, tools, commands or maths.",
        Some(open.upcast_ref()),
    ));
    page.append(&intro);

    page.append(&label("What you can type", &["section-title"]));
    let examples = card();
    examples.add_css_class("flush");
    examples.set_spacing(0);
    let rows = [
        ("view-app-grid-symbolic", "firefox", "Search your apps and quiver's tools. Typos are fine: \"vsc\" finds Visual Studio Code."),
        ("accessories-calculator-symbolic", "2*21  or  =sqrt(2)", "Quick maths. Enter copies the result."),
        ("utilities-terminal-symbolic", ">htop", "Lists only commands on your PATH. Shift+Enter runs it in a terminal."),
        ("input-keyboard-symbolic", "↑ ↓  Tab  Enter  Esc", "Move through results, open the selected one, or close."),
    ];
    for (icon_name, example, what) in rows {
        let chip = gtk::Label::builder().label(example).css_classes(["mono", "dim"]).build();
        examples.append(&setting_row(icon_name, what, "", Some(chip.upcast_ref())));
    }
    page.append(&examples);
    page
}
