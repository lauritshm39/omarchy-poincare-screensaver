//! Running one orbit and turning it into dots.

use crate::canvas::{Canvas, Rgb};
use crate::orbits::Orbit;
use crate::palette::{self, Palette};
use crate::physics::{System, Vec2};
use std::collections::VecDeque;

/// World coordinates to dot coordinates. Braille dots are near enough square at
/// the font sizes involved, so one scale serves both axes.
#[derive(Clone, Copy)]
pub struct View {
    scale: f64,
    wcx: f64,
    wcy: f64,
    dcx: f64,
    dcy: f64,
}

impl View {
    pub fn fit(bbox: (f64, f64, f64, f64), x0: f64, y0: f64, w: f64, h: f64, margin: f64) -> View {
        let (minx, miny, maxx, maxy) = bbox;
        let (ww, wh) = ((maxx - minx).max(1e-6), (maxy - miny).max(1e-6));
        let scale = ((w * (1.0 - margin)) / ww).min((h * (1.0 - margin)) / wh);
        View {
            scale,
            wcx: (minx + maxx) * 0.5,
            wcy: (miny + maxy) * 0.5,
            dcx: x0 + w * 0.5,
            dcy: y0 + h * 0.5,
        }
    }

    pub fn dot(&self, p: Vec2) -> (f64, f64) {
        (
            self.dcx + (p.x - self.wcx) * self.scale,
            // Screen y grows downward; the orbits are nicer the right way up.
            self.dcy - (p.y - self.wcy) * self.scale,
        )
    }
}

/// How heavy the drawing is. Both scale with the canvas so that raising the
/// resolution makes the picture finer rather than merely thinner.
#[derive(Clone, Copy)]
pub struct Style {
    /// Radius of the trail brush, in dots. 0 is a single-dot line.
    pub stroke: i32,
    /// Radius of a body's glow, in dots.
    pub body: i32,
}

impl Style {
    pub fn auto(dots_h: usize, stroke: Option<f64>, body: Option<f64>) -> Style {
        let h = dots_h as f64;
        Style {
            stroke: stroke.unwrap_or((h / 170.0).clamp(1.0, 5.0)).round() as i32,
            body: body.unwrap_or((h / 28.0).clamp(4.0, 20.0)).round() as i32,
        }
    }
}

pub struct Sim {
    pub orbit: &'static Orbit,
    sys: System,
    view: View,
    /// Trail points already in dot coordinates, deduplicated. Storing them this
    /// way keeps the buffer small and the redraw cheap; it is safe because the
    /// view is fixed for the lifetime of the sim.
    trails: [VecDeque<(i32, i32)>; 3],
    heads: [(f64, f64); 3],
    /// Simulation time to cover per rendered frame.
    per_frame: f64,
    max_trail: usize,
    /// Simulation time run since the last restart, and the span past which this
    /// orbit's published initial conditions stop being faithful.
    elapsed: f64,
    usable: f64,
}

/// Ceiling on the work one frame may ask of the adaptive stepper. Hitting it
/// slows an orbit down rather than dropping frames, which is the right way for
/// a screensaver to fail.
const MAX_STEPS_PER_FRAME: u32 = 40_000;

impl Sim {
    pub fn new(
        orbit: &'static Orbit,
        x0: f64,
        y0: f64,
        w: f64,
        h: f64,
        margin: f64,
        periods: f64,
        seconds: f64,
        fps: f64,
        max_trail: usize,
    ) -> Sim {
        let profile = crate::orbits::profile(orbit);
        let view = View::fit(profile.bbox, x0, y0, w, h, margin);
        let sys = orbit.system();

        let per_frame = (orbit.period * periods / seconds.max(0.1)) / fps.max(1.0);

        let heads = [view.dot(sys.pos[0]), view.dot(sys.pos[1]), view.dot(sys.pos[2])];
        Sim {
            orbit,
            sys,
            view,
            trails: [VecDeque::new(), VecDeque::new(), VecDeque::new()],
            heads,
            per_frame,
            max_trail,
            elapsed: 0.0,
            usable: profile.usable,
        }
    }

    pub fn advance(&mut self) {
        // Six samples a frame is already smoother than the dot grid can show.
        let sample_interval = self.per_frame / 6.0;
        let mut next_sample = sample_interval;
        let mut done = 0.0;
        let mut steps = 0u32;

        while done < self.per_frame && steps < MAX_STEPS_PER_FRAME {
            let dt = self
                .sys
                .suggested_dt(crate::orbits::MAX_DT, crate::orbits::MIN_DT)
                .min(self.per_frame - done);
            self.sys.step(dt);
            done += dt;
            self.elapsed += dt;
            steps += 1;

            if self.elapsed >= self.usable {
                // Replay from the initial conditions. For an orbit that really
                // closes this is invisible -- the curve retraces itself. For
                // the long-period entries the catalogue cannot pin down, it is
                // what stops a body being flung off the screen.
                self.sys = self.orbit.system();
                self.elapsed = 0.0;
            }
            if done >= next_sample {
                next_sample += sample_interval;
                self.sample();
            }
        }
        self.sample();
    }

    fn sample(&mut self) {
        for i in 0..3 {
            let (x, y) = self.view.dot(self.sys.pos[i]);
            self.heads[i] = (x, y);
            let p = (x.round() as i32, y.round() as i32);
            let t = &mut self.trails[i];
            if t.back() != Some(&p) {
                t.push_back(p);
                if t.len() > self.max_trail {
                    t.pop_front();
                }
            }
        }
    }

    pub fn draw(&self, c: &mut Canvas, pal: &Palette, style: Style) {
        for i in 0..3 {
            let col = pal.bodies[i].trail;
            let t = &self.trails[i];
            let n = t.len();
            // The reference keeps the whole closed curve visible at a low,
            // roughly even brightness and brightens sharply near the body. The
            // bright run is measured against the canvas so it stays the same
            // visible length whatever the resolution.
            let hot = (c.dots_h() * 13 / 10).max(160);
            for k in 1..n {
                let age = n - k;
                let b = if age < hot {
                    0.55 + 0.45 * (1.0 - age as f32 / hot as f32).powf(2.0)
                } else {
                    0.55
                };
                let (a, z) = (t[k - 1], t[k]);
                // Fading toward the background rather than toward black is what
                // lets the same code work on a light theme.
                c.line(a.0, a.1, z.0, z.1, palette::mix(pal.background, col, b), b, style.stroke);
            }
            glow(c, self.heads[i].0, self.heads[i].1, &pal.bodies[i], pal.background, style.body);
        }
    }

    /// Every trail dot, as morph sources.
    pub fn source_points(&self, out: &mut Vec<(f64, f64, Rgb)>, pal: &Palette) {
        for i in 0..3 {
            let col = pal.bodies[i].trail;
            for &(x, y) in &self.trails[i] {
                out.push((x as f64, y as f64, col));
            }
        }
    }
}

/// A body: white-hot centre with a small coloured halo. Braille gives one
/// colour per cell, so the halo has to be built from neighbouring dots.
/// A body: a solid bright core inside a soft halo of its own hue.
///
/// A single falloff curve does not work here. Braille gives one colour per
/// cell, so "brightness" is only ever a colour mixed toward the background, and
/// a smooth falloff spends most of its radius nearly invisible -- enlarging it
/// then adds halo nobody can see rather than a bigger body. A flat core with a
/// shorter falloff around it is what actually reads as a glowing point.
pub fn glow(c: &mut Canvas, x: f64, y: f64, col: &palette::BodyColour, bg: Rgb, r: i32) {
    let (cx, cy) = (x.round() as i32, y.round() as i32);
    let radius = r.max(1) as f32;
    let core = (radius * 0.42).max(1.0);

    for dy in -r..=r {
        for dx in -r..=r {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if d > radius {
                continue;
            }
            let t = if d <= core {
                1.0
            } else {
                (1.0 - (d - core) / (radius - core + 0.8)).clamp(0.0, 1.0).powf(1.5)
            };
            if t < 0.04 {
                continue;
            }
            let colour = palette::mix(col.trail, col.core, t);
            c.dot(
                cx + dx,
                cy + dy,
                palette::mix(bg, colour, 0.40 + 0.60 * t),
                // Weighted well above the trail so a body crossing its own tail
                // still reads as the bright thing in that cell.
                2.0 + t * 10.0,
            );
        }
    }
}

