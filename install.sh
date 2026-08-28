#!/usr/bin/env bash
#
# Installs the Poincare three-body screensaver into ~/.local/bin.
#
# Nothing under /usr/share/omarchy is touched: that directory belongs to the
# omarchy package and is replaced on every update. The screensaver reuses
# Omarchy's own window class and terminal configs by reading them, not by
# modifying anything.

set -euo pipefail

BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
IDLE_HOOK=0
UNINSTALL_IDLE=0

usage() {
  cat <<USAGE
Usage: ./install.sh [--idle] [--remove-idle]

  (no flags)      Build and install the binary and launcher into $BIN_DIR.
  --idle          Additionally point Omarchy's idle timer at this screensaver
                  instead of the stock one. Does this the supported way: clones
                  the built-in omarchy.idle shell plugin into your own config
                  and edits the clone, so an omarchy update cannot undo it and
                  nothing system-owned is modified.
  --remove-idle   Undo --idle: re-enable the built-in idle service and delete
                  the clone.
USAGE
}

while (($# > 0)); do
  case "$1" in
  --idle) IDLE_HOOK=1 ;;
  --remove-idle) UNINSTALL_IDLE=1 ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    echo "unknown option: $1" >&2
    usage >&2
    exit 2
    ;;
  esac
  shift
done

here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
clone_id="${USER:-$(id -un)}.idle"
clone_dir="$HOME/.config/omarchy/plugins/$clone_id"

if ((UNINSTALL_IDLE)); then
  if [[ -d $clone_dir ]]; then
    omarchy plugin enable omarchy.idle >/dev/null 2>&1 || true
    omarchy plugin disable "$clone_id" >/dev/null 2>&1 || true
    rm -rf "$clone_dir"
    omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
    echo "Restored the built-in idle service and removed $clone_dir."
  else
    echo "No idle clone at $clone_dir; nothing to undo."
  fi
  exit 0
fi

command -v cargo >/dev/null || {
  echo "cargo not found. Install Rust first: pacman -S rust (or mise use -g rust@stable)" >&2
  exit 1
}

echo "Building..."
cargo build --release --manifest-path "$here/Cargo.toml"

mkdir -p "$BIN_DIR"
install -m755 "$here/target/release/omarchy-poincare" "$BIN_DIR/omarchy-poincare"
install -m755 "$here/bin/omarchy-poincare-screensaver" "$BIN_DIR/omarchy-poincare-screensaver"
install -m755 "$here/bin/omarchy-launch-poincare-screensaver" "$BIN_DIR/omarchy-launch-poincare-screensaver"
echo "Installed into $BIN_DIR."

case ":$PATH:" in
*":$BIN_DIR:"*) ;;
*) echo "WARNING: $BIN_DIR is not on your PATH." >&2 ;;
esac

if ((IDLE_HOOK)); then
  if [[ -d $clone_dir ]]; then
    echo "Idle clone already exists at $clone_dir; leaving it alone."
  else
    echo "Cloning the built-in idle service..."
    omarchy plugin clone omarchy.idle >/dev/null
  fi

  service="$clone_dir/Service.qml"
  [[ -f $service ]] || {
    echo "Expected $service after cloning; aborting." >&2
    exit 1
  }
  if grep -q 'omarchy-launch-poincare-screensaver' "$service"; then
    echo "Idle service already points at the Poincare screensaver."
  else
    grep -q 'omarchy-launch-screensaver' "$service" || {
      echo "Could not find the launcher call in $service; Omarchy may have changed." >&2
      echo "Point it at omarchy-launch-poincare-screensaver by hand." >&2
      exit 1
    }
    sed -i 's/omarchy-launch-screensaver/omarchy-launch-poincare-screensaver/g' "$service"
    omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
    echo "Idle service now launches the Poincare screensaver."
  fi
fi

cat <<DONE

Try it now (a whole cycle in a few seconds, in this terminal):
    omarchy-poincare --sim 3 --once

Or as Omarchy launches it (the logo arrives after the 10s orbit phase):
    omarchy-launch-poincare-screensaver
    omarchy-poincare --list                 # the orbit library
    omarchy-poincare --verify               # check every orbit really is periodic
    omarchy-poincare --layout grid          # the whole library at once

Settings live in ~/.config/omarchy-poincare/config
(timings, orbit, colours, resolution, stroke weight -- see docs/configuration.md).

Bind a key (in ~/.config/hypr/bindings.lua):
    o.bind("SUPER SHIFT", "S", "Screensaver", "omarchy-launch-poincare-screensaver")
DONE
