# The orbit library

[← README](../README.md) · [Configuration](configuration.md) · [Internals](internals.md)

In 1890 Poincaré showed that the three-body problem has no general closed-form
solution and depends sensitively on its initial conditions. What does exist is a
catalogue of isolated *periodic* orbits, and that catalogue is what makes a
screensaver out of it possible.

18 orbits. `omarchy-screensaver-poincare --list` prints them; `--orbit <key>` picks one.

| family | orbits |
|---|---|
| classical | Lagrange equilateral triangle, Euler collinear, figure eight |
| butterfly | I, II, III, IV\* |
| moth | I, II, III, bumblebee, dragonfly |
| goggles | goggles |
| yarn | yarn |
| yin-yang | I (a), I (b), II (a)\*, II (b)\* |

\* excluded from random selection — see below.

Lagrange's and Euler's solutions are derived in code from their own geometry
rather than transcribed, so they are exact by construction. The figure eight is
Chenciner–Montgomery. The rest come from the Šuvakov–Dmitrašinović
classification of planar equal-mass periodic orbits,
[PRL 110, 114301](https://arxiv.org/abs/1303.0181), which parameterises them as

```
r1 = (-1, 0)   r2 = (1, 0)   r3 = (0, 0)
v1 = v2 = (vx, vy)           v3 = (-2vx, -2vy)
```

with G = m = 1, so the centre of mass starts at rest at the origin.

## `--verify`

Published initial conditions are transcriptions, and a chaotic system is
unforgiving of a wrong digit. `--verify` integrates every orbit for exactly one
period and reports how far it lands from where it started:

```
KEY                 PERIOD      DRIFT    ENERGY  USABLE      VIEW BOX  VERDICT
lagrange            10.260    4.71e-8   2.8e-14    100%   2.54x2.54    closes
figure-eight         6.326    2.14e-7   5.0e-14    100%   2.38x0.79    closes
moth-i              14.894    1.44e-4   1.7e-12    100%   2.36x2.42    closes
butterfly-iv        79.476     1.08e0    3.4e-8    100%   2.24x1.57    does not close
```

- **DRIFT** — largest per-body displacement from the starting configuration after
  one period, relative to that configuration's own size.
- **ENERGY** — relative change in total energy over the same integration.
- **USABLE** — fraction of a period before a body is flung away. The screensaver
  replays only this much.
- **VIEW BOX** — the extent the view is fitted to.

Because `lagrange` and `euler` are derived rather than transcribed, they close to
machine precision and act as a check on the integrator itself. If they ever stop
closing, the integrator is wrong, not the data.

`OMARCHY_POINCARE_VERIFY_DT` overrides the step ceiling, which separates a bad
transcription from an under-resolved close approach.

## What verification found

Two transcription errors, caught before they ever reached the screen: a mistyped
digit in Butterfly IV's `vy`, and a mislabelled pair of yin-yang orbits — what
was filed as Yin-Yang II (a)/(b) was really I (b) and II (a), and the true
II (b) was missing entirely.

One integrator bug. With a fixed timestep, **Butterfly II ejected a body and flew
off the screen**; energy was not conserved even at a very fine step. The step is
now adaptive, and it closes to `1.5e-3`.

And one thing no amount of care could fix. **Butterfly IV, Yin-Yang II (a) and
Yin-Yang II (b) cannot be reproduced from their published five- and six-digit
initial conditions.** Over their 54–79 time-unit periods, chaos amplifies the
truncation until the orbit no longer closes — it is the data that runs out of
precision, not the arithmetic. They stay bound and still look like orbits, so
they remain selectable by `--orbit`, but random selection skips them.

`--verify` re-measures every entry and **exits non-zero** if the list in
`orbits::UNRELIABLE` disagrees with what it measures, so the catalogue cannot
quietly drift away from the truth.
