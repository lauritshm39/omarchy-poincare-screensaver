# omarchy-screensavers

A collection of screensavers for [Omarchy](https://omarchy.org), sharing one
terminal renderer, one morph engine and one launcher.

- **poincare**: a periodic solution of the Newtonian three-body problem
  traced out in the terminal, which then collapses into the Omarchy logo —
  or any name you like.
- **starry** (coming soon): a twinkling starfield that gathers into the logo.

In 1890 Poincaré showed the three-body problem has no general closed-form
solution and depends sensitively on its initial conditions. What does exist is a
catalogue of isolated *periodic* orbits, and that catalogue is what this draws.

- **[Configuration](docs/configuration.md)** — the config file, every option,
  timings, colours, resolution and stroke weight, and
  [choosing which screensaver runs](docs/configuration.md#choosing-which-screensaver-runs)
- **[The orbit library](docs/orbits.md)** — the 18 orbits, and `--verify`
- **[Internals](docs/internals.md)** — how it is built, why, and the tests

## Install

Needs a Rust toolchain (`pacman -S rust`, or `mise use -g rust@stable`).

```bash
./install.sh              # build, install into ~/.local/bin
./install.sh --idle       # ...and make the idle timer use it (see below)
./install.sh --remove-idle
```

Then bind a key in `~/.config/hypr/bindings.lua`:

```lua
o.bind("SUPER SHIFT", "S", "Screensaver", "omarchy-launch-screensavers")
```

### Hooking the idle timer

Omarchy's idle service starts the screensaver by calling
`omarchy-launch-screensaver` by name, and `/usr/share/omarchy/bin` comes first
on `PATH`, so a shadowing script will not win. Editing that directory is also
the wrong answer — it belongs to the package and is replaced on every update.

The supported route is to clone the idle service into your own config and edit
the clone, which is what `--idle` does:

```bash
omarchy plugin clone omarchy.idle
sed -i 's/omarchy-launch-screensaver/omarchy-launch-screensavers/' \
  ~/.config/omarchy/plugins/$USER.idle/Service.qml
```

The clone lives in `~/.config/omarchy/plugins/` and survives `omarchy update`.
Nothing system-owned is touched.

## Use

```bash
omarchy-screensaver-poincare --sim 3 --once        # a whole cycle in a few seconds
omarchy-screensaver-poincare --orbit figure-eight  # a specific simulation
omarchy-screensaver-poincare --layout grid         # every orbit at once
omarchy-screensaver-poincare --text "LAURITS"      # morph into a name instead
omarchy-screensaver-poincare --list                # the orbit library
omarchy-screensaver-poincare --palette             # colours resolved from your theme
omarchy-screensaver-poincare --verify              # check every orbit really is periodic
```

Settings live in `~/.config/omarchy-screensavers/config`, since the launcher passes
no arguments of its own — see **[Configuration](docs/configuration.md)**.

The cycle is orbit (10s) → collapse into the logo (2.6s) → hold (5s) →
scatter (1.6s) → next random orbit.

## Tests

```bash
cargo test --release    # 72 tests, about a second
```

`omarchy-screensaver-poincare --verify` is the other check worth running: it re-measures
every orbit and exits non-zero if the catalogue disagrees with the physics.
See [Internals](docs/internals.md#tests).

## Why Rust

The screensaver replaces the program inside Omarchy's existing screensaver
terminal, so it inherits the launcher, the window rules, the fonts and the
exit-on-keypress behaviour, and changes only the drawing.

- Omarchy already migrated this exact workload from Python (`tte`) to Rust
  (`ttfx`), so a Rust binary swims with the current.
- Startup latency is visible: the terminal maps and something must paint *now*.
  A static binary starts in about a millisecond; an interpreter plus numeric
  imports costs a few hundred, once per monitor.
- It runs while the machine is idle, often on battery, on every monitor at once,
  and integrating a chaotic system needs small steps and `f64`.

On an i7-6700K at 300x84 cells and 60fps: **5% of one core**, **6.6 MB**
resident, one dependency (`crossterm`, plus `libc` for one call). Numbers across
resolutions are in [Configuration](docs/configuration.md#resolution).

## Credits

The look — amber, periwinkle and warm white on black, thin fading trails,
glowing bodies — is modelled on
[this animation by Massimo](https://x.com/Rainmaker1973/status/2092828199399149582).

Orbit data from Šuvakov & Dmitrašinović,
*Three Classes of Newtonian Three-Body Planar Periodic Orbits*,
[PRL 110, 114301](https://arxiv.org/abs/1303.0181).
