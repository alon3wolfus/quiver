mod calc;
mod launcher;
mod ocr;
mod picker;
mod rename;
mod resize;
mod top;
mod util;
mod zones;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// A boredom driven PowerToys replacement for Linux/Hyprland.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
    Run,
    /// Toggle "always on top" for the focused window
    Top,
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
    match Cli::parse().command {
        Command::Pick { format } => picker::pick(&format),
        Command::Ocr { lang } => ocr::extract(&lang),
        Command::Zones { action } => zones::run(action),
        Command::Run => launcher::run(),
        Command::Top => top::toggle(),
        Command::Rename { find, replace, files, regex, ignore_case, yes } => {
            rename::run(rename::Options { find, replace, files, regex, ignore_case, yes })
        }
        Command::Resize { files, size, overwrite } => resize::run(&files, &size, overwrite),
    }
}
