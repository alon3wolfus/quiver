# quiver
a boredom driven microsoft powertoys replacement for linux/hyprland

One binary, seven tools, and a settings window for all of them:

| Command | PowerToys equivalent | What it does |
| --- | --- | --- |
| `quiver` / `quiver settings` | PowerToys Settings | Settings window with a page for every tool |
| `quiver run` | PowerToys Run | Spotlight-style launcher for apps, quiver's tools, `=` maths and `>` shell commands |
| `quiver zones` | FancyZones | Snap windows into predefined layouts |
| `quiver rename` | PowerRename | Bulk-rename files with find & replace, regex and counters |
| `quiver top` | Always On Top | Pin the focused window above everything else |
| `quiver pick` | Color Picker | Click any pixel, its colour is copied (`--format hex/rgb/hsl/hsv/cmyk`) |
| `quiver ocr` | Text Extractor | Drag a box over the screen, the text inside is copied (`--lang eng+deu`) |
| `quiver resize` | Image Resizer | Shrink images to a preset, exact size or percentage |

Needs Hyprland 0.55 or newer (the Lua config version).

## Install

```bash
sudo pacman -S --needed rustup gtk4 gtk4-layer-shell hyprpicker grim slurp wl-clipboard tesseract tesseract-data-eng libnotify
rustup default stable
cargo install --path .
```

This puts `quiver` in `~/.cargo/bin`. For fish, add that to your PATH with `fish_add_path ~/.cargo/bin`.

## Hyprland keybinds

Add to your Hyprland Lua config (with caelestia, `~/.config/caelestia/hypr-user.lua`). These keys are free in a default caelestia setup; check `hyprctl binds` for clashes with your own.

```lua
local quiver = os.getenv("HOME") .. "/.cargo/bin/quiver"

-- Keep the launcher loaded in the background so ALT + SPACE opens it instantly
hl.on("hyprland.start", function()
    hl.exec_cmd(quiver .. " run --daemon")
end)

hl.bind("ALT + SPACE", hl.dsp.exec_cmd(quiver .. " run"))                  -- Launcher
hl.bind("SUPER + ALT + C", hl.dsp.exec_cmd(quiver .. " pick"))             -- Colour picker
hl.bind("SUPER + SHIFT + T", hl.dsp.exec_cmd(quiver .. " ocr"))            -- Text extractor
hl.bind("SUPER + CTRL + T", hl.dsp.exec_cmd(quiver .. " top"))             -- Always on top

-- Zones
hl.bind("SUPER + CTRL + Z", hl.dsp.exec_cmd(quiver .. " zones apply main-stack"))
hl.bind("SUPER + CTRL + G", hl.dsp.exec_cmd(quiver .. " zones apply grid"))
-- caelestia binds SUPER + ALT + arrows to resizing; free them in hypr-vars.lua first
hl.bind("SUPER + ALT + Left", hl.dsp.exec_cmd(quiver .. " zones snap columns 1"))
hl.bind("SUPER + ALT + Right", hl.dsp.exec_cmd(quiver .. " zones snap columns 2"))
hl.bind("SUPER + ALT + Up", hl.dsp.exec_cmd(quiver .. " zones snap focus 1"))

-- Launcher pops in with frosted-glass blur; ignore_alpha keeps the blur off the
-- transparent space around the bar
hl.layer_rule({ match = { namespace = "quiver" }, animation = "popin 80%", blur = true, ignore_alpha = 0.2 })
```

## Settings window (`quiver`)

Run `quiver` with no command (or `quiver settings`, or search "settings" in the launcher). Like PowerToys Settings, it has:

- **Home**: Hyprland connection status, quick access to every tool, all your quiver keybinds, and a utilities list
- **Colour Picker**: pick a colour, see recent picks, shades, and HEX / RGB / HSL / HSV / CMYK values with copy buttons
- **Rename**: find & replace with a live original → renamed preview, then Apply
- **Zones**: every layout drawn as a card; click one to tile your workspace, or edit the layouts file
- **Always On Top**: pin or unpin your open windows
- **Text Extractor**, **Image Resizer** and **Launcher** pages

Each tool page shows the keybinds it found in your Hyprland Lua config. The window follows your GTK theme's colours.

## Launcher (`quiver run`)

The launcher stays loaded in the background after the first use, so it opens in well under 0.1 s (GTK takes over a second to start from scratch). `quiver run` asks the background launcher to show itself and exits immediately; pressing the key again while it's open hides it. `quiver run --daemon` starts it hidden, e.g. at login, so even the first open is instant.

A floating, Spotlight-style search bar with app icons. The round buttons next to it limit results to apps, quiver's tools, commands or maths; click one again to go back to everything. With blur on (see the `layer_rule` above) it looks like frosted glass.

- Type to fuzzy-search installed apps (`vsc` finds Visual Studio Code) and quiver's own tools (`color`, `ocr`, `pin`, `zones`)
- Type maths like `2*21` or `=sqrt(2)`, Enter copies the result
- Type `>` to list only the commands on your PATH, e.g. `>htop` or `>ls -la`
  - Enter runs the command, Shift+Enter runs it in your terminal and keeps it open
- Up/Down or Tab to move, Enter to launch, Escape to close

## Zones (`quiver zones`)

Built-in layouts: `columns`, `thirds`, `main-stack`, `grid`, `focus`.

- `quiver zones list` shows them
- `quiver zones apply <layout>` tiles every window on the current workspace, most recently focused first
- `quiver zones snap <layout> <n>` moves just the focused window into zone `n`
- Add `--dry-run` to print the Hyprland commands instead of running them

Windows placed in zones become floating. To make your own layouts, run `quiver zones init` and edit `~/.config/quiver/layouts.toml`:

```toml
gap = 10
[layouts]
# [x, y, width, height] in percent of the screen, excluding bars
wide-left = [[0, 0, 70, 100], [70, 0, 30, 100]]
```

## Rename (`quiver rename`)

Shows a preview first. Nothing changes until you add `--yes`.

```bash
quiver rename draft final *.txt
quiver rename --regex '^IMG_(\d+)' 'trip-{n}-$1' *.jpg --yes
```

- `{n}` becomes a counter (1, 2, 3...), and with `--regex`, `$1` is the first capture group
- `--ignore-case` matches regardless of upper/lower case
- Refuses to overwrite existing files or give two files the same name

## Always On Top (`quiver top`)

Pins the focused window so it stays above other windows and follows you to every workspace. Run it again to unpin.

Hyprland can only pin floating windows, so a tiled window is floated exactly where its tile was while pinned. Unpinning puts it back into the tiling layout on the workspace you're on. Windows that were already floating stay floating.

## Resize (`quiver resize`)

```bash
quiver resize photo.jpg
quiver resize *.png --size 50%
quiver resize scan.jpg --size 1200x1200 --overwrite
```

- Sizes: `small` (854×480), `medium` (1366×768, the default), `large` (1920×1080), `phone` (320×568), `WIDTHxHEIGHT` or `N%`
- Keeps the aspect ratio and never enlarges with a fixed size
- Saves next to the original as `photo (Medium).jpg`, unless you pass `--overwrite`
