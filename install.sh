#!/usr/bin/env bash
#
# Installs the omarchy-screensavers collection into ~/.local/bin.
#
# Nothing under /usr/share/omarchy is touched: that directory belongs to the
# omarchy package and is replaced on every update. The screensavers reuse
# Omarchy's own window class and terminal configs by reading them, not by
# modifying anything.

set -euo pipefail

BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
IDLE_HOOK=0
UNINSTALL_IDLE=0

usage() {
  cat <<USAGE
Usage: ./install.sh [--idle] [--remove-idle]

  (no flags)      Build and install the binaries and launchers into $BIN_DIR.
  --idle          Additionally point Omarchy's idle timer at these screensavers
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
install -m755 "$here/target/release/omarchy-screensaver-poincare" "$BIN_DIR/omarchy-screensaver-poincare"
install -m755 "$here/target/release/omarchy-screensaver-starry" "$BIN_DIR/omarchy-screensaver-starry"
install -m755 "$here/bin/omarchy-screensaver-run" "$BIN_DIR/omarchy-screensaver-run"
install -m755 "$here/bin/omarchy-launch-screensavers" "$BIN_DIR/omarchy-launch-screensavers"
install -m755 "$here/bin/omarchy-launch-poincare" "$BIN_DIR/omarchy-launch-poincare"
install -m755 "$here/bin/omarchy-launch-starry" "$BIN_DIR/omarchy-launch-starry"

# Backwards compatibility with the single-screensaver layout: the old names
# stay as thin shims for one release so existing keybindings, the existing
# idle hook (Service.qml) and the old config path keep working. New installs
# should use the names above.
cat >"$BIN_DIR/omarchy-launch-poincare-screensaver" <<'SHIM'
#!/bin/bash
# Deprecated shim: use omarchy-launch-screensavers instead.
exec omarchy-launch-screensavers poincare "$@"
SHIM
chmod 755 "$BIN_DIR/omarchy-launch-poincare-screensaver"
cat >"$BIN_DIR/omarchy-poincare-screensaver" <<'SHIM'
#!/bin/bash
# Deprecated shim: use omarchy-screensaver-run instead.
exec omarchy-screensaver-run omarchy-screensaver-poincare "$@"
SHIM
chmod 755 "$BIN_DIR/omarchy-poincare-screensaver"
if [[ ! -e "$BIN_DIR/omarchy-poincare" ]]; then
  ln -s omarchy-screensaver-poincare "$BIN_DIR/omarchy-poincare"
fi
echo "Installed into $BIN_DIR (old poincare names kept as shims)."

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
  if grep -q 'omarchy-launch-screensavers' "$service"; then
    echo "Idle service already points at the screensavers launcher."
  else
    if grep -q 'omarchy-launch-poincare-screensaver' "$service"; then
      sed -i 's/omarchy-launch-poincare-screensaver/omarchy-launch-screensavers/g' "$service"
    elif grep -q 'omarchy-launch-screensaver' "$service"; then
      sed -i 's/omarchy-launch-screensaver/omarchy-launch-screensavers/g' "$service"
    else
      echo "Could not find the launcher call in $service; Omarchy may have changed." >&2
      echo "Point it at omarchy-launch-screensavers by hand." >&2
      exit 1
    fi
    omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
    echo "Idle service now launches the screensavers collection."
  fi
fi

cat <<DONE

Try it now (a whole cycle in a few seconds, in this terminal):
    omarchy-screensaver-poincare --sim 3 --once

Or as Omarchy launches it (the logo arrives after the 10s orbit phase):
    omarchy-launch-screensavers                 # whichever --screensaver selects
    omarchy-launch-poincare                     # just the three-body simulation
    omarchy-launch-starry                       # just the starfield
    omarchy-screensaver-poincare --list      # the orbit library
    omarchy-screensaver-poincare --verify    # check every orbit really is periodic
    omarchy-screensaver-poincare --layout grid  # the whole library at once

Settings live in ~/.config/omarchy-screensavers/config
(old ~/.config/omarchy-poincare/config still works as a fallback;
timings, orbit, colours, resolution, stroke weight -- see docs/configuration.md).

Bind a key (in ~/.config/hypr/bindings.lua):
    o.bind("SUPER SHIFT", "S", "Screensaver", "omarchy-launch-screensavers")
DONE
