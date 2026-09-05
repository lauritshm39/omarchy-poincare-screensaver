//! Running one orbit and turning it into dots.

use crate::orbits::Orbit;
use crate::physics::{System, Vec2};
use omarchy_screensaver_core::canvas::{Canvas, Rgb};
use omarchy_screensaver_core::palette::{self, Palette};
use omarchy_screensaver_core::stage::{self, GatherPhase, Style};
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

/// How heavy the drawing is lives in core now ([`Style`][core_style]); this
/// module keeps only the orbit-specific view fitting.
///
/// [core_style]: omarchy_screensaver_core::stage::Style

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
            stage::glow(c, self.heads[i].0, self.heads[i].1, &pal.bodies[i], pal.background, style.body);
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

impl GatherPhase for Sim {
    fn advance(&mut self) {
        Sim::advance(self);
    }

    fn draw(&self, canvas: &mut Canvas, pal: &Palette, style: Style) {
        Sim::draw(self, canvas, pal, style);
    }

    fn source_points(&self, out: &mut Vec<(f64, f64, Rgb)>, pal: &Palette) {
        Sim::source_points(self, out, pal);
    }
}

