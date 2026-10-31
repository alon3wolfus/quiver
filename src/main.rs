mod calc;
mod color;
mod display;
mod draw;
mod files;
mod gui;
mod launcher;
mod locksmith;
mod ocr;
mod overlay;
mod picker;
mod rename;
mod resize;
mod ruler;
mod top;
mod util;
mod zones;
mod zoomit;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// A boredom driven PowerToys replacement for Linux/Hyprland.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// With no command, the settings window opens
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Pick a colour from the screen and copy it to the clipboard
    Pick {
        /// Output format: hex, rgb, hsl, hsv or cmyk
        #[arg(short, long, default_value = "hex")]
        format: String,
    },
    /// Select a screen region and copy the text inside it (OCR)
    Ocr {
        /// Tesseract language(s), e.g. "eng" or "eng+deu"
        #[arg(short, long, default_value = "eng")]
        lang: String,
    },
    /// Arrange windows into predefined layouts
    Zones {
        #[command(subcommand)]
        action: zones::Action,
    },
    /// Open the app launcher / calculator / command runner
    Run {
        /// Start the launcher hidden in the background (e.g. at login) so opening it is instant
        #[arg(long)]
        daemon: bool,
    },
    /// Open the settings window with every tool
    Settings {
        /// Page to open, e.g. display, zones, picker
        page: Option<String>,
    },
    /// Measure distances on the screen (Screen Ruler)
    Ruler,
    /// Draw on a frozen screenshot, then copy or save it (ZoomIt)
    Draw,
    /// Zoom the screen around the mouse (ZoomIt)
    Zoom {
        #[arg(value_enum, default_value = "toggle")]
        action: zoomit::ZoomAction,
    },
    /// Start or stop recording the screen to ~/Videos (ZoomIt)
    Record {
        /// Drag to choose the area to record
        #[arg(long)]
        region: bool,
    },
    /// Show which processes are using files or folders (File Locksmith)
    Locksmith {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Toggle "always on top" for the focused window (tiled windows return to the grid on unpin)
    Top {
        /// Target this window address instead of the focused one
        #[arg(long, hide = true)]
        address: Option<String>,
    },
    /// Bulk-rename files with find & replace (shows a preview unless --yes)
    Rename {
        /// Text (or regex with --regex) to find in each file name
        find: String,
        /// Replacement; {n} becomes a counter, $1 a regex capture group
        replace: String,
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Treat FIND as a regular expression
        #[arg(short, long)]
        regex: bool,
        /// Match regardless of upper/lower case
        #[arg(short, long)]
        ignore_case: bool,
        /// Actually rename instead of only previewing
        #[arg(short, long)]
        yes: bool,
    },
    /// Resize images, saved next to the original as "name (Size).ext"
    Resize {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// small, medium, large, phone, WIDTHxHEIGHT or a percentage like 50%
        #[arg(short, long, default_value = "medium")]
        size: String,
        /// Replace the original files instead of writing copies
        #[arg(long)]
        overwrite: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let Some(command) = Cli::parse().command else { return gui::run(None) };
    match command {
        Command::Pick { format } => picker::pick(&format),
        Command::Ocr { lang } => ocr::extract(&lang),
        Command::Zones { action } => zones::run(action),
        Command::Run { daemon } => launcher::run(daemon),
        Command::Settings { page } => gui::run(page),
        Command::Ruler => ruler::run(),
        Command::Draw => draw::run(),
        Command::Zoom { action } => zoomit::zoom(action).map(|z| println!("zoom {}%", (z * 100.0).round())),
        Command::Record { region } => zoomit::record(region).map(|msg| println!("{msg}")),
        Command::Locksmith { paths } => locksmith::run(&paths),
        Command::Top { address } => top::toggle(address.as_deref()),
        Command::Rename { find, replace, files, regex, ignore_case, yes } => {
            rename::run(rename::Options { find, replace, files, regex, ignore_case, yes })
        }
        Command::Resize { files, size, overwrite } => resize::run(&files, &size, overwrite),
    }
}
