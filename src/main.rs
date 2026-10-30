mod calc;
mod launcher;
mod ocr;
mod picker;
mod util;
mod zones;

use clap::{Parser, Subcommand};

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
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Pick { format } => picker::pick(&format),
        Command::Ocr { lang } => ocr::extract(&lang),
        Command::Zones { action } => zones::run(action),
        Command::Run => launcher::run(),
    }
}
