#!/bin/sh
# Installs the latest quiver release on Arch Linux or Debian/Ubuntu, with its dependencies:
#
#   curl -fsSL https://raw.githubusercontent.com/alon3wolfus/quiver/main/install.sh | sh
#
# It downloads the .pkg.tar.zst (Arch) or .deb (Debian/Ubuntu) from the latest GitHub
# release and installs it with pacman or apt, so it's removed the normal way later.
set -eu

repo=alon3wolfus/quiver

say() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# Asks a yes/no question even when this script is piped into sh (stdin is the script).
ask() {
    if [ -r /dev/tty ]; then
        printf '%s [Y/n] ' "$1" > /dev/tty
        read -r answer < /dev/tty || answer=
    else
        answer=y
    fi
    case "$answer" in [nN]*) return 1 ;; *) return 0 ;; esac
}

[ "$(uname -m)" = x86_64 ] || die "releases are x86_64 only. Build from source instead: https://github.com/$repo#from-source"
command -v curl > /dev/null || die "curl is needed"

sudo=
if [ "$(id -u)" -ne 0 ]; then
    command -v sudo > /dev/null || die "run this as root or install sudo"
    sudo=sudo
fi

# The latest release's version, from where github.com/.../releases/latest redirects
# (no API call, so no rate limit).
latest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")
version=${latest##*/v}
case "$version" in
    [0-9]*) ;;
    *) die "couldn't find a quiver release. Build from source instead: https://github.com/$repo#from-source" ;;
esac

# pacman's questions must read the terminal, not this script coming in on stdin.
if [ -r /dev/tty ]; then
    pacman_in=/dev/tty
    pacman_flags=
else
    pacman_in=/dev/null
    pacman_flags=--noconfirm
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
# apt reads the file as its own sandbox user.
chmod 755 "$tmp"

download() {
    file="$tmp/$1"
    say "Downloading $1"
    curl -fL --progress-bar -o "$file" "https://github.com/$repo/releases/download/v$version/$1" \
        || die "release v$version has no $1. Build from source instead: https://github.com/$repo#from-source"
}

if command -v pacman > /dev/null; then
    download "quiver-$version-1-x86_64.pkg.tar.zst"
    say "Installing with pacman (pulls in gtk4, gtk4-layer-shell, hyprpicker, grim, slurp, wl-clipboard, tesseract)"
    $sudo pacman -U --needed $pacman_flags "$file" < "$pacman_in"
    extras="tesseract-data-eng libnotify gpu-screen-recorder plocate"
    if ask "Also install the extras (English OCR data, notifications, screen recording, whole-system file search)?"; then
        $sudo pacman -S --needed --asdeps $pacman_flags $extras < "$pacman_in"
        $sudo updatedb || true
    else
        say "Skipped. Install later with: sudo pacman -S --asdeps $extras"
    fi
elif command -v apt-get > /dev/null; then
    download "quiver_$version-1_amd64.deb"
    say "Installing with apt (needs Debian 13 or Ubuntu 25.04 or newer)"
    $sudo apt-get install -y "$file" < /dev/null \
        || die "apt couldn't install it. quiver needs libgtk4-layer-shell0 and glibc 2.39, from Debian 13 / Ubuntu 25.04 or newer."
    $sudo updatedb 2> /dev/null || true
else
    die "this script supports Arch (pacman) and Debian/Ubuntu (apt). Build from source instead: https://github.com/$repo#from-source"
fi

say "Installed $(quiver --version)"
cat << 'EOF'

Next steps:
  quiver               open the settings window (or find "Quiver" in your app menu)
  quiver run           open the launcher

Add the keybinds from the README to your Hyprland Lua config, including
  hl.on("hyprland.start", function() hl.exec_cmd("quiver run --daemon") end)
so the launcher opens instantly:
  https://github.com/alon3wolfus/quiver#hyprland-keybinds
EOF
