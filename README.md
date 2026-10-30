# quiver
a boredom driven microsoft powertoys replacement for linux/hyprland

One binary, four tools:

| Command | PowerToys equivalent | What it does |
| --- | --- | --- |
| `quiver pick` | Color Picker | Click any pixel, its colour is copied (`--format hex/rgb/hsl/hsv/cmyk`) |
| `quiver ocr` | Text Extractor | Drag a box over the screen, the text inside is copied (`--lang eng+deu`) |
| `quiver zones` | FancyZones | Snap windows into predefined layouts |
| `quiver run` | PowerToys Run | Launcher for apps, `=` maths and `>` shell commands |

## Install

```bash
sudo pacman -S --needed rustup gtk4 gtk4-layer-shell hyprpicker grim slurp wl-clipboard tesseract tesseract-data-eng libnotify
rustup default stable
cargo install --path .
```

## Hyprland keybinds

Add to `~/.config/hypr/hyprland.conf`:

```ini
bind = SUPER SHIFT, C, exec, quiver pick
bind = SUPER SHIFT, T, exec, quiver ocr
bind = ALT, SPACE, exec, quiver run
bind = SUPER, Z, exec, quiver zones apply main-stack
bind = SUPER ALT, left, exec, quiver zones snap columns 1
bind = SUPER ALT, right, exec, quiver zones snap columns 2

# Optional: blur behind the launcher
layerrule = blur, quiver
```

## Launcher

- Type to fuzzy-search installed apps (`vsc` finds Visual Studio Code)
- Type maths like `2*21` or `=sqrt(2)`, Enter copies the result
- `>command` runs a shell command
- Up/Down or Tab to move, Enter to launch, Escape to close

## Zones

Built-in layouts: `columns`, `thirds`, `main-stack`, `grid`, `focus`.

- `quiver zones list` shows them
- `quiver zones apply <layout>` tiles every window on the current workspace, most recently focused first
- `quiver zones snap <layout> <n>` moves just the focused window into zone `n`
- Add `--dry-run` to print the `hyprctl` commands instead of running them

Windows placed in zones become floating. To make your own layouts, run `quiver zones init` and edit `~/.config/quiver/layouts.toml`:

```toml
gap = 10
[layouts]
# [x, y, width, height] in percent of the screen, excluding bars
wide-left = [[0, 0, 70, 100], [70, 0, 30, 100]]
```
