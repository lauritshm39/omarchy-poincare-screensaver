//! The library of three-body simulations.
//!
//! Poincare proved the three-body problem has no general closed-form solution
//! and is chaotic; what *does* exist is a growing catalogue of isolated
//! *periodic* solutions, which is what makes a screensaver possible. Most
//! entries below come from the Suvakov-Dmitrasinovic classification of planar
//! equal-mass periodic orbits (Phys. Rev. Lett. 110, 114301, 2013), which uses
//! a two-parameter initial condition:
//!
//!     r1 = (-1, 0)   r2 = (1, 0)   r3 = (0, 0)
//!     v1 = v2 = (vx, vy)           v3 = (-2vx, -2vy)
//!
//! with G = m = 1, so the centre of mass starts at rest at the origin.
//!
//! The published (vx, vy, T) values are transcriptions, and a chaotic system is
//! unforgiving of a wrong digit. `omarchy-screensaver-poincare --verify` integrates each
//! orbit for exactly one period and reports how far it lands from its starting
//! state, so every entry here can be checked rather than trusted.

use crate::physics::{System, Vec2};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Step bounds shared by the profiler and the running screensaver, so what is
/// measured is what is drawn.
pub const MAX_DT: f64 = 2.0e-4;
pub const MIN_DT: f64 = 1.0e-8;

pub struct Orbit {
    pub key: &'static str,
    pub name: &'static str,
    pub family: &'static str,
    /// One period, in the same time units as the integration.
    pub period: f64,
    pub init: Init,
}

pub enum Init {
    /// Suvakov-Dmitrasinovic two-parameter form (equal masses).
    Sd { vx: f64, vy: f64 },
    /// Positions and velocities given outright, with masses.
    Explicit { pos: [[f64; 2]; 3], vel: [[f64; 2]; 3], mass: [f64; 3] },
    /// Equilateral triangle rotating rigidly (Lagrange, 1772). Derived, not
    /// transcribed, so it doubles as a correctness baseline for the integrator.
    Lagrange,
    /// Collinear rotating configuration (Euler, 1767). Also derived.
    Euler,
}

impl Orbit {
    pub fn system(&self) -> System {
        match self.init {
            Init::Sd { vx, vy } => System::new(
                [Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 0.0)],
                [Vec2::new(vx, vy), Vec2::new(vx, vy), Vec2::new(-2.0 * vx, -2.0 * vy)],
                [1.0, 1.0, 1.0],
            ),
            Init::Explicit { pos, vel, mass } => System::new(
                [
                    Vec2::new(pos[0][0], pos[0][1]),
                    Vec2::new(pos[1][0], pos[1][1]),
                    Vec2::new(pos[2][0], pos[2][1]),
                ],
                [
                    Vec2::new(vel[0][0], vel[0][1]),
                    Vec2::new(vel[1][0], vel[1][1]),
                    Vec2::new(vel[2][0], vel[2][1]),
                ],
                mass,
            ),
            Init::Lagrange => {
                // Equal masses on a circle of radius R, triangle side a = R*sqrt(3).
                // Net inward pull is sqrt(3)/a^2 per body, so w^2 = 3/a^3.
                let a = 2.0_f64;
                let r = a / 3.0_f64.sqrt();
                let w = (3.0 / (a * a * a)).sqrt();
                let mut pos = [Vec2::default(); 3];
                let mut vel = [Vec2::default(); 3];
                for i in 0..3 {
                    let th = std::f64::consts::TAU * i as f64 / 3.0;
                    pos[i] = Vec2::new(r * th.cos(), r * th.sin());
                    vel[i] = Vec2::new(-w * r * th.sin(), w * r * th.cos());
                }
                System::new(pos, vel, [1.0, 1.0, 1.0])
            }
            Init::Euler => {
                // Bodies at -x, 0, +x rotating rigidly. The outer body feels
                // 1/x^2 from the centre and 1/(2x)^2 from the far body, so
                // w^2 = 1.25 / x^3. The middle body is in equilibrium.
                let x = 1.0_f64;
                let w = (1.25 / (x * x * x)).sqrt();
                System::new(
                    [Vec2::new(-x, 0.0), Vec2::new(0.0, 0.0), Vec2::new(x, 0.0)],
                    [Vec2::new(0.0, -w * x), Vec2::new(0.0, 0.0), Vec2::new(0.0, w * x)],
                    [1.0, 1.0, 1.0],
                )
            }
        }
    }
}

pub const ORBITS: &[Orbit] = &[
    // --- Derived analytically; exact by construction. -----------------------
    Orbit {
        key: "lagrange",
        name: "Lagrange equilateral triangle",
        family: "classical",
        period: std::f64::consts::TAU / 0.612_372_435_695_794_5, // TAU / sqrt(3/8)
        init: Init::Lagrange,
    },
    Orbit {
        key: "euler",
        name: "Euler collinear",
        family: "classical",
        period: std::f64::consts::TAU / 1.118_033_988_749_895, // TAU / sqrt(1.25)
        init: Init::Euler,
    },
    // --- Chenciner-Montgomery / Moore figure eight. -------------------------
    Orbit {
        key: "figure-eight",
        name: "Figure eight",
        family: "classical",
        period: 6.325_913_98,
        init: Init::Explicit {
            pos: [
                [0.970_004_36, -0.243_087_53],
                [-0.970_004_36, 0.243_087_53],
                [0.0, 0.0],
            ],
            vel: [
                [0.466_203_685, 0.432_365_73],
                [0.466_203_685, 0.432_365_73],
                [-0.932_407_37, -0.864_731_46],
            ],
            mass: [1.0, 1.0, 1.0],
        },
    },
    // --- Suvakov-Dmitrasinovic catalogue. -----------------------------------
    Orbit { key: "butterfly-i",   name: "Butterfly I",   family: "butterfly", period: 6.235_6,  init: Init::Sd { vx: 0.306_890, vy: 0.125_510 } },
    Orbit { key: "butterfly-ii",  name: "Butterfly II",  family: "butterfly", period: 7.003_9,  init: Init::Sd { vx: 0.392_950, vy: 0.097_580 } },
    Orbit { key: "butterfly-iii", name: "Butterfly III", family: "butterfly", period: 13.865_8, init: Init::Sd { vx: 0.405_920, vy: 0.230_160 } },
    Orbit { key: "butterfly-iv",  name: "Butterfly IV",  family: "butterfly", period: 79.475_9, init: Init::Sd { vx: 0.350_112, vy: 0.079_340 } },
    Orbit { key: "moth-i",        name: "Moth I",        family: "moth",      period: 14.893_9, init: Init::Sd { vx: 0.464_440, vy: 0.396_060 } },
    Orbit { key: "moth-ii",       name: "Moth II",       family: "moth",      period: 28.670_3, init: Init::Sd { vx: 0.439_170, vy: 0.452_970 } },
    Orbit { key: "moth-iii",      name: "Moth III",      family: "moth",      period: 25.840_6, init: Init::Sd { vx: 0.383_440, vy: 0.377_360 } },
    Orbit { key: "bumblebee",     name: "Bumblebee",     family: "moth",      period: 63.534_5, init: Init::Sd { vx: 0.184_280, vy: 0.587_190 } },
    Orbit { key: "dragonfly",     name: "Dragonfly",     family: "moth",      period: 21.271_0, init: Init::Sd { vx: 0.080_580, vy: 0.588_840 } },
    Orbit { key: "goggles",       name: "Goggles",       family: "goggles",   period: 10.466_8, init: Init::Sd { vx: 0.083_300, vy: 0.127_890 } },
    Orbit { key: "yarn",          name: "Yarn",          family: "yarn",      period: 55.501_8, init: Init::Sd { vx: 0.559_060, vy: 0.349_190 } },
    Orbit { key: "yin-yang-ia",   name: "Yin-Yang I (a)",  family: "yin-yang", period: 17.328_4, init: Init::Sd { vx: 0.513_940, vy: 0.304_740 } },
    Orbit { key: "yin-yang-ib",   name: "Yin-Yang I (b)",  family: "yin-yang", period: 10.962_6, init: Init::Sd { vx: 0.282_700, vy: 0.327_210 } },
    Orbit { key: "yin-yang-iia",  name: "Yin-Yang II (a)", family: "yin-yang", period: 55.789_8, init: Init::Sd { vx: 0.416_820, vy: 0.330_330 } },
    Orbit { key: "yin-yang-iib",  name: "Yin-Yang II (b)", family: "yin-yang", period: 54.207_6, init: Init::Sd { vx: 0.417_340, vy: 0.313_100 } },
];

/// Orbits whose published initial conditions do not survive a full period at
/// double precision. Measured, not assumed -- `--verify` recomputes the usable
/// span for every entry and complains if this list disagrees with it. They stay
/// selectable by name, but random selection skips them so the screensaver never
/// shows a body being flung off the screen.
pub const UNRELIABLE: &[&str] = &["butterfly-iv", "yin-yang-iia", "yin-yang-iib"];

pub fn is_reliable(o: &Orbit) -> bool {
    !UNRELIABLE.contains(&o.key)
}

pub fn reliable() -> Vec<&'static Orbit> {
    ORBITS.iter().filter(|o| is_reliable(o)).collect()
}

pub fn find(key: &str) -> Option<&'static Orbit> {
    ORBITS.iter().find(|o| o.key.eq_ignore_ascii_case(key))
}

/// What an orbit actually looks like when integrated, and for how long it can
/// be trusted.
///
/// The published initial conditions carry five or six significant digits. For
/// the short orbits that is plenty, but a chaotic system amplifies truncation
/// exponentially, and the T = 54..79 members of the catalogue disintegrate --
/// bodies are ejected -- well before one period is out. Rather than drop them
/// or pretend, we measure the span over which the trajectory is still faithful
/// (energy conserved, nothing escaping) and the screensaver replays only that.
#[derive(Clone)]
pub struct Profile {
    /// Bounding box (min x, min y, max x, max y) over the usable span.
    pub bbox: (f64, f64, f64, f64),
    /// Simulation time before the trajectory stops being trustworthy.
    pub usable: f64,
}

pub fn profile(orbit: &Orbit) -> Profile {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, Profile>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(p) = cache.lock().ok().and_then(|c| c.get(orbit.key).cloned()) {
        return p;
    }

    let mut s = orbit.system();
    let size0 = s.pos.iter().map(|p| p.norm()).fold(1e-9_f64, f64::max);

    // Sampled positions, from which the view box is taken as a percentile
    // rather than an outright min/max. A single wide excursion -- moth III has
    // one -- would otherwise set the scale for the whole orbit and shrink the
    // part you actually want to look at to a speck.
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();

    let sample_interval = orbit.period / 20_000.0;
    let mut next_sample = 0.0;
    let mut t = 0.0;
    let mut usable = orbit.period;
    // Bounds the work an adaptive step can demand of a pathological orbit.
    let mut budget = 4_000_000u32;

    while t < orbit.period {
        // Checked before recording, so one bad state cannot blow up the view.
        //
        // Instantaneous energy would be the obvious test and is the wrong one:
        // a symplectic integrator lets energy swing during a close approach and
        // brings it back, so a faithful orbit trips it. What actually ruins the
        // picture is a body being flung away, so that is what we watch for.
        let com = s.centre_of_mass();
        let far = s.pos.iter().map(|p| (*p - com).norm()).fold(0.0_f64, f64::max);
        if !far.is_finite() || far > 6.0 * size0 || budget == 0 {
            usable = t;
            break;
        }
        if t >= next_sample {
            next_sample += sample_interval;
            for p in &s.pos {
                xs.push(p.x);
                ys.push(p.y);
            }
        }
        let dt = s.suggested_dt(MAX_DT, MIN_DT).min(orbit.period - t);
        s.step(dt);
        t += dt;
        budget -= 1;
    }

    let (minx, maxx) = span(&mut xs);
    let (miny, maxy) = span(&mut ys);

    let profile = Profile {
        // View::fit applies one scale to both axes, so the aspect is preserved
        // without padding this out to a square -- which would leave a wide
        // orbit like the figure eight marooned in the middle of the screen at a
        // third of the size it could be.
        bbox: (minx, miny, maxx, maxy),
        // Never below a token span, or a broken entry would flicker.
        usable: usable.max(orbit.period * 0.05),
    };
    if let Ok(mut c) = cache.lock() {
        c.insert(orbit.key, profile.clone());
    }
    profile
}

/// The range holding the central 96% of a set of coordinates.
///
/// Not the full extent: several orbits -- moth III most sharply -- spend almost
/// all of their time in a compact region and then make one long excursion. Fit
/// the full extent and the part worth looking at shrinks to a speck. Fitting
/// the bulk instead lets the rare excursion run off the edge, which costs
/// nothing: the canvas discards dots outside it.
fn span(v: &mut Vec<f64>) -> (f64, f64) {
    if v.is_empty() {
        return (-1.0, 1.0);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let q = |f: f64| v[((v.len() - 1) as f64 * f).round() as usize];
    let (lo, hi) = (q(0.02), q(0.98));
    let pad = (hi - lo).max(1e-6) * 0.05;
    (lo - pad, hi + pad)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lagrange's and Euler's solutions are derived from their own geometry in
    /// code rather than transcribed, so they must close to near machine
    /// precision. If these fail, the integrator is wrong, not the data.
    #[test]
    fn derived_orbits_close_to_machine_precision() {
        for key in ["lagrange", "euler"] {
            let o = find(key).unwrap();
            assert!(
                closure_drift(o) < 1e-6,
                "{key} drifted {:e}",
                closure_drift(o)
            );
        }
    }

    #[test]
    fn figure_eight_closes() {
        assert!(closure_drift(find("figure-eight").unwrap()) < 1e-5);
    }

    /// The reliability list is a claim about measured behaviour. This is the
    /// check that stops it drifting away from the truth, and the reason two
    /// transcription errors were caught before they reached the screen.
    #[test]
    fn unreliable_list_matches_what_is_measured() {
        for o in ORBITS {
            let drift = closure_drift(o);
            let usable = profile(o).usable / o.period;
            let good = drift < 1e-1 && usable > 0.99;
            assert_eq!(
                good,
                is_reliable(o),
                "{}: drift {drift:e}, usable {:.0}% -- UNRELIABLE disagrees",
                o.key,
                usable * 100.0
            );
        }
    }

    /// Every orbit must stay on screen for the span the screensaver replays,
    /// including the ones whose published data does not close.
    #[test]
    fn every_orbit_stays_bounded_over_its_usable_span() {
        for o in ORBITS {
            let p = profile(o);
            let (w, h) = (p.bbox.2 - p.bbox.0, p.bbox.3 - p.bbox.1);
            assert!(w.is_finite() && h.is_finite(), "{}: non-finite view box", o.key);
            assert!(w > 0.1 && w < 20.0, "{}: view box width {w}", o.key);
            assert!(h > 0.1 && h < 20.0, "{}: view box height {h}", o.key);
            assert!(p.usable > 0.0, "{}: no usable span", o.key);
        }
    }

    /// A single wide excursion must not set the scale for the whole orbit --
    /// moth III spends nearly all its time in a compact region and then makes
    /// one long trip, which once shrank it to a speck.
    #[test]
    fn view_box_is_not_dominated_by_a_rare_excursion() {
        let p = profile(find("moth-iii").unwrap());
        let h = p.bbox.3 - p.bbox.1;
        assert!(h < 3.0, "moth-iii view box height {h} -- excursion is dominating again");
    }

    #[test]
    fn keys_are_unique_and_lookup_is_case_insensitive() {
        let mut keys: Vec<&str> = ORBITS.iter().map(|o| o.key).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n, "duplicate orbit keys");
        assert!(find("FIGURE-EIGHT").is_some());
        assert!(find("no-such-orbit").is_none());
    }

    #[test]
    fn random_selection_pool_excludes_the_unreliable_ones() {
        let pool = reliable();
        assert!(!pool.is_empty());
        for key in UNRELIABLE {
            assert!(!pool.iter().any(|o| o.key == *key), "{key} is in the random pool");
        }
    }

    /// Integrates one period with the same adaptive stepper the screensaver
    /// uses, and reports the largest per-body displacement from the start.
    fn closure_drift(orbit: &Orbit) -> f64 {
        let start = orbit.system();
        let mut s = start.clone();
        let mut t = 0.0;
        while t < orbit.period {
            let dt = s.suggested_dt(MAX_DT, MIN_DT).min(orbit.period - t);
            s.step(dt);
            t += dt;
        }
        let scale = start.pos.iter().map(|p| p.norm()).fold(1e-9_f64, f64::max);
        (0..3)
            .map(|i| (s.pos[i] - start.pos[i]).norm() / scale)
            .fold(0.0_f64, f64::max)
    }
}
