# Configuration

[← README](../README.md) · [Orbits](orbits.md) · [Internals](internals.md)

The launcher passes no arguments of its own, so settings live in
`~/.config/omarchy-screensavers/config`. One option per line, `#` starts a comment,
and an option's value is the rest of its line — so `--text MY NAME` needs no
quoting. Anything typed on the command line overrides the file.

```
# ~/.config/omarchy-screensavers/config
--font-size 8             # resolution of the picture
--sim 10                  # seconds of orbit before it collapses into the logo
# --orbit figure-eight    # commented out: pick at random
```

A mistake in this file is reported and skipped rather than being fatal — the
idle timer launches the screensaver with no terminal to read an error from, and
a stray character is no reason for it to stop appearing:

```
omarchy-screensaver-poincare: ignoring config: --sim expects a number, got "banana"
```

Typing the same mistake on the command line does stop, since someone is there
to read it.

## Choosing which screensaver runs

Omarchy has no notion of more than one screensaver — a single hardcoded program
and an on/off toggle — so the choosing happens in this project's launcher, which
is what the idle timer calls.

```
--screensaver poincare    # the three-body simulation (default)
--screensaver starry      # the starfield
--screensaver omarchy     # hand off to Omarchy's stock ttfx screensaver
--screensaver random      # one of the three, chosen afresh on every launch
```

`random` re-rolls per launch, so an idle machine alternates between them. With
`omarchy`, the launcher execs `omarchy-launch-screensaver` and everything after
that is stock behaviour.

The mode can also be given as an argument, which overrides the config file:

```bash
omarchy-launch-screensavers omarchy    # stock, just this once
omarchy-launch-screensavers random
```

Or launch one screensaver directly, bypassing the chooser entirely:

```bash
omarchy-launch-poincare                # just the three-body simulation
omarchy-launch-starry                  # just the starfield
```

## Options

`omarchy-screensaver-poincare --help` and `omarchy-screensaver-starry --help`
are authoritative; this is the same list with context. Both binaries read the
same config file; each accepts (and ignores) the other's specific options, so
`--orbit figure-eight` alongside `--stars 1500` warns nowhere whichever binary
runs.

### Orbit (poincare only)

| option | default | |
|---|---|---|
| `--orbit <key\|random>` | `random` | Which simulation to run. Keys from `--list`. |
| `--layout <single\|grid>` | `single` | One orbit, or the whole library tiled at once. |
| `--periods <n>` | `2` | Orbit periods drawn per orbit phase. Higher redraws the curve more times in the same wall-clock. |

### Sky (starry only)

| option | default | |
|---|---|---|
| `--stars <n>` | canvas-scaled | How many stars. Defaults scale with the area, 400..4000. |
| `--star-size <x>` | `1.2` | Star disc multiplier. Near stars stamp small discs; far stars stay single dots. `0` keeps every star a single dot. |
| `--speed <x>` | `6.0` | Outward drift multiplier. |
| `--warp <x>` | `3.0` | Hyperdrive gain: extra outward speed with distance from centre. `0` is a calm sky. |
| `--black-hole` / `--no-black-hole` | on | Reveal the art over a black hole with a glowing accretion ring. During the gather phase the hole is already there: stars swirl around it, nothing is drawn over its disc, and swallowed stars respawn upfield. Meteors flare out when they hit it. |
| `--hole-size <x>` | `0.3` | Black-hole radius as a fraction of the smaller canvas dimension. |

### What it morphs into

| option | default | |
|---|---|---|
| `--file <path>` | Omarchy branding | Art to morph into. |
| `--text <string>` | — | Draw a string with the built-in 5x7 block font instead. |
| `--text-scale <n>` | `1` | Enlarge `--text`. |
| `--art-scale <auto\|n>` | `auto` | Enlarge block art to suit the terminal. |

### Which screensaver

| option | default | |
|---|---|---|
| `--screensaver <poincare\|starry\|omarchy\|random>` | `poincare` | Read by the launcher — see above. |

By default the target is `~/.config/omarchy/branding/screensaver.txt` — the same
file `omarchy branding screensaver` manages, falling back to the stock logo. Set
it once and both this and Omarchy's own screensaver follow:

```bash
omarchy branding screensaver image   # transcode a PNG/SVG to block art
omarchy branding screensaver text    # edit the art by hand
```

`--text` exists because a short name read as a file would be seven characters
lost in the middle of the screen; the block font brings it up to logo weight.

`--art-scale auto` enlarges block art by whole-number factors so the logo holds
roughly the same fraction of the screen whatever the font size. It only applies
to art that is actually made of block-drawing characters — text art is left
alone, since enlarging it would turn it to mush.

### Timing

| phase | option | default | |
|---|---|---|---|
| gather | `--sim <s>` | `10` poincare / `8` starry | orbit traced (or stars drift), sources accumulate |
| morph | `--morph <s>` | `2.6` | every source dot spirals in to a glyph cell |
| hold | `--hold <s>` | `5` | logo settles from the gathered colours to the theme foreground |
| scatter | `--scatter <s>` | `1.6` | flies apart, then a new round |

Plus `--fps <n>` (default `60`).

To watch a whole cycle immediately, without waiting out the gather phase:

```bash
omarchy-screensaver-poincare --sim 3 --once
omarchy-screensaver-starry --sim 3 --once
```

### Appearance

| option | default | |
|---|---|---|
| `--font-size <n>` | `18` | Terminal font size, and so the resolution. Applied by the launcher. |
| `--stroke <n>` | auto | Trail thickness, radius in dots. |
| `--body <n>` | auto | Body size, glow radius in dots. |
| `--theme <auto\|off\|dir>` | `auto` | Follow the active Omarchy theme. |

### Other

`--once` runs a single cycle and exits. `--managed` suppresses input handling
because the launcher owns the keyboard; you would not normally set either.

`--list`, `--palette` and `--verify` print and exit.

## Colours

The palette comes from whatever Omarchy theme is currently applied. `omarchy
theme set` re-stages the active theme into a fixed directory, which is re-read
at the start of every cycle — so switching themes takes effect within one orbit,
without restarting anything.

```bash
omarchy-screensaver-poincare --palette          # show the colours resolved from your theme
omarchy-screensaver-poincare --theme off        # the built-in amber/periwinkle palette
omarchy-screensaver-poincare --theme <dir>      # read a specific theme directory
```

Three body colours are chosen from the theme's accents by greedy maximum hue
separation, then nudged into a lightness band where thin glowing curves read
clearly against the background. The logo settles to the theme's foreground, and
trails fade toward the theme's background rather than toward black — which is
what lets this work on a light theme as well as a dark one.

Some themes have no hue separation to give: `vantablack` and `white` are
effectively monochrome. Those are detected, and the bodies are spread across
lightness instead, that being the only axis such a theme leaves. A theme that
declares no `mode` has it inferred from its background.

A very desaturated theme produces correspondingly muted bodies. That is faithful,
but if you want the punchier look of the reference animation, `--theme off` gives
amber, periwinkle and warm white on black.

## Resolution

Detail comes from the terminal font size. The picture is drawn in braille
sub-cells, 2x4 dots per character, so halving the font doubles the detail in
each direction. The screensaver adapts to whatever size the terminal is; the
launcher sets it from `--font-size`.

Measured on an i7-6700K at 1920x1080, one orbit at 60fps, screensaver process
only:

| font | cells | dots | CPU |
|---|---|---|---|
| 18 (Omarchy's default) | 133x35 | 266x140 | 2.6% of one core |
| 12 | 200x50 | 400x200 | 2.9% |
| 9 | 266x70 | 532x280 | 3.1% |
| 8 | 300x84 | 600x336 | 3.6% |
| 6 | 400x112 | 800x448 | 4.7% |
| 4.5 | 533x140 | 1066x560 | 5.2% |

Sixteen times the pixel area costs twice the CPU: the integration dominates and
does not depend on resolution at all, and trail drawing scales with the length
of a curve rather than the area it encloses. Memory stays around 6.6 MB
throughout.

Two caveats. Those figures are for the thin default stroke — the heavier
defaults now in use cost about 5.0% at 300x84. And the terminal emulator's own
work is on top of this and is not measured: output rises from about 320 KB/s at
a one-dot stroke to about 1 MB/s at the current defaults. If the screensaver
ever feels less than smooth, try `--stroke 1` first, then a larger
`--font-size`.

Everything with a size scales with the canvas, so a smaller font makes the
picture finer rather than merely thinner: trail thickness, body size, the bright
leading run of a trail, the trail length cap, and block art.

```
--stroke 2      # trail thickness, radius in dots
--body 12       # body size, glow radius in dots
```
