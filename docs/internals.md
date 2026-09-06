# Internals

[← README](../README.md) · [Configuration](configuration.md) · [Orbits](orbits.md)

The cycle is gather → collapse → hold → scatter → next round. Poincare's
gather phase runs orbits; starry's drifts a starfield. Everything after the
gather is the shared stage machine.

| module | |
|---|---|
| `crates/poincare/{physics,orbits,sim}.rs` | the three-body system, the orbit library, the orbit runner |
| `crates/starry/starry.rs` | the warp starfield gather phase |
| `crates/core/{canvas,morph,target,palette,rng}.rs` | shared braille canvas, morph, art, colours, PRNG |
| `crates/core/stage.rs` | the shared gather → morph → hold → scatter phase machine, plus the black-hole backdrop |
| `crates/core/config.rs` | the shared config file |

## Tests

```bash
cargo test --release      # 72 tests, ~1s
cargo test                # same, ~12s (the physics tests are integration-heavy)
```

They concentrate on the places that actually produced bugs during development,
not on coverage for its own sake:

- **`physics`** — a two-body circular orbit whose period follows from Newton
  alone, so the integrator is checked against arithmetic rather than itself;
  energy conservation over 500k adaptive steps; the centre of mass staying put;
  and that `suggested_dt` really does shrink during a close approach, which is
  the whole reason it exists.
- **`orbits`** — that the derived orbits close to machine precision, and that
  `UNRELIABLE` still matches what integration measures. That last one is the
  guard that caught two transcription errors; it fails the build if the
  catalogue drifts from the truth.
- **`target`** — that re-encoding scaled art preserves coverage, that the
  scaler only emits exactly representable cells, that text art is never
  enlarged, and that `fit_scale` never overflows the canvas.
- **`palette`** — hue separation on a normal theme, lightness separation on a
  monochrome one, and light/dark inference from the background. All three were
  broken at some point.
- **`main`** — config parsing: trailing comments, `#` inside a value,
  command-line precedence, and that a broken config is skipped rather than
  fatal. Two of these shipped as bugs.
- **`canvas`** — braille bit mapping, intensity weighting, and that the diffing
  renderer emits nothing when nothing changed and a space when a cell is
  cleared.
- **`morph`** — that particles start on a source and land on a target, that
  every glyph cell is claimed, and that mismatched cloud sizes still pair up.
- **`starry`** — that the star count scales with the canvas and stays bounded,
  that twinkle stays in range, that warp recycles stars so none escape, that
  warp moves stars outward while a calm sky barely moves, that disc radius
  grows with size and layer, that sources cover the canvas rather than
  clumping, and that far stars outnumber near ones.

One test is worth knowing about: `every_option_in_the_help_text_is_actually_accepted`
parses `--help` and feeds every flag it mentions back through the argument
parser, so the help text cannot document an option that does not exist.

## Integration

Yoshida 4th-order symplectic. Energy error stays bounded over the hours a
screensaver may run, which Runge–Kutta would not manage.

The step is adaptive. The local dynamical time goes as `r^(3/2)`, so scaling the
step that way costs nothing while the bodies are apart and automatically buys
resolution when they are not. This is not a refinement — with a fixed step sized
for the leisurely part of an orbit, a close approach injects enough energy to
eject a body, which is exactly what happened to Butterfly II.

Instantaneous energy would be the obvious way to detect a trajectory falling
apart, and it is the wrong one: a symplectic integrator lets energy swing during
a close approach and brings it back, so a perfectly faithful orbit trips it. The
profiler watches for a body being flung away instead.

## Fitting the view

Each orbit is profiled once and cached: integrate a period, record where the
bodies go, and fit the view to the **central 96%** of that, not its full extent.
Several orbits — moth III most sharply — spend nearly all their time in a
compact region and then make one long excursion. Fit the full extent and the
part worth looking at shrinks to a speck; fit the bulk and the rare excursion
runs off the edge, which costs nothing because the canvas discards dots outside
it.

## Drawing

Each terminal cell holds a 2x4 grid of braille dots. At the font sizes involved
those dots are close to square, so world coordinates map to dot coordinates with
a single scale.

A cell carries one foreground colour, so the colours of dots landing in it are
averaged, weighted by intensity — that is how a bright body crossing its own
faint tail still reads as the bright thing in that cell.

Trails are drawn as line segments, not points, because a body near periapsis
crosses many dots between two sampled positions. Thickness comes from stamping a
bar perpendicular to each line's major axis rather than a disc at every step: a
disc is `O(r²)` per step and almost entirely overlaps the previous one. At these
trail lengths that was most of the drawing cost — switching to bars halved the
CPU for an identical picture.

A body is a flat bright core inside a short falloff, not one smooth curve.
Braille gives one colour per cell, so brightness is only ever a colour mixed
toward the background, and a smooth falloff spends most of its radius nearly
invisible — enlarging it adds halo nobody can see rather than a bigger body.

The renderer repaints only the cells that changed. A full repaint of a large
terminal is several hundred KB; at 60fps that alone would keep a core busy,
which is the wrong thing for something that runs while the machine is idle.

## The morph

Pairing two point clouds optimally is an assignment problem, and Hungarian
matching on a few thousand points per morph is far too slow for a frame budget.
Both clouds are sorted by angle about their own centroid and zipped, which costs
`O(n log n)`, rarely crosses paths, and — because the interpolation is done in
polar coordinates — reads as the orbit spiralling inward and settling into the
glyphs.

Each particle carries a small random delay so the logo does not snap into
existence all at once, and each glyph appears as its particle lands, wearing
that body's colour, before settling to the theme foreground.

## Scaling block art

Art is enlarged by expanding each character into its 2x4 sub-cell coverage,
scaling that bitmap, and mapping every 2x4 block of the result back to the
closest block-drawing character by Hamming distance. Because the source is
axis-aligned rectangles, whole-number scaling stays crisp.
