//! A twinkling starfield with a hyperdrive warp, doubling as gather-phase
//! sources.
//!
//! Stars stream radially outward from the screen centre, accelerating as they
//! go, each drawn with a motion streak and a per-layer disc size. Twinkle
//! survives underneath. When the stage collects [`Starfield::source_points`],
//! the current star positions become the morph sources, so the warp collapses
//! into the branding art -- revealed over a black hole -- through the shared
//! [`Morph`][crate::morph::Morph].

use omarchy_screensaver_core::canvas::{Canvas, Rgb};
use omarchy_screensaver_core::palette::{self, Palette};
use omarchy_screensaver_core::rng::Rng;
use omarchy_screensaver_core::stage::{GatherCtx, GatherPhase, Style};

/// One star, in dot coordinates.
struct Star {
    x: f64,
    y: f64,
    /// Previous position, for the motion streak.
    px: f64,
    py: f64,
    /// Parallax layer: 0 is far (slow, small), 2 is near (fast, big).
    layer: usize,
    /// Base brightness before twinkling, in (0, 1].
    base: f32,
    /// Twinkle phase and speed (radians and radians/second).
    phase: f64,
    twinkle: f64,
    /// Which palette entry this star wears: 0..3 index into bodies, or
    /// `usize::MAX` for the logo resting colour (near-white).
    colour: usize,
}

/// An occasional shooting star: a bright head with a fading tail.
struct Meteor {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    life: f64,
    max_life: f64,
}

/// Tuning for the starfield. All three are configurable; see `--help`.
#[derive(Clone, Copy)]
pub struct StarCfg {
    /// Star disc multiplier. Layer radii (0/0.9/1.6 dots) scale with this.
    pub size: f64,
    /// Base outward speed multiplier.
    pub speed: f64,
    /// Hyperdrive gain: extra outward speed per dot of distance from centre.
    /// 0 is a calm drifting sky; ~1 is full warp.
    pub warp: f64,
}

impl Default for StarCfg {
    fn default() -> Self {
        StarCfg { size: 1.2, speed: 6.0, warp: 3.0 }
    }
}

/// Disc radius for a layer at a size multiplier. Far stars stay single dots;
/// mid and near stars stamp small discs.
fn star_radius(layer: usize, size: f64) -> i32 {
    let base = match layer {
        0 => 0.0,
        1 => 0.9,
        _ => 1.6,
    };
    ((base * size).round() as i32).clamp(0, 3)
}

pub struct Starfield {
    stars: Vec<Star>,
    dw: f64,
    dh: f64,
    cfg: StarCfg,
    /// Black-hole geometry for this cycle. `None` when `--no-black-hole`.
    hole: Option<GatherCtx>,
    /// Seconds since creation, advanced once per frame.
    age: f64,
    dt: f64,
    meteor: Option<Meteor>,
    /// Seconds until the next meteor may spawn.
    meteor_in: f64,
}

impl Starfield {
    /// `count` overrides the area-scaled default. `fps` sets the per-frame
    /// time step driving drift and twinkle. `hole` is this cycle's black-hole
    /// geometry (or `None` with `--no-black-hole`): stars spawn outside it and
    /// swirl around it.
    pub fn new(
        dw: f64,
        dh: f64,
        rng: &mut Rng,
        count: Option<usize>,
        fps: f64,
        cfg: StarCfg,
        hole: Option<GatherCtx>,
    ) -> Starfield {
        let n = count.unwrap_or_else(|| default_count(dw, dh));
        // Typical per-frame travel of a near-layer star at the rim, so spawns
        // clear the hole with room to spare. Over-estimating just pushes the
        // exclusion zone out a little; under-estimating pops stars onto the rim.
        let clearance = rim_speed(dh, cfg) / fps.max(1.0);
        let mut stars = Vec::with_capacity(n);
        for _ in 0..n {
            let layer = weighted_layer(rng);
            let (x, y) = spawn_point(dw, dh, hole.as_ref(), clearance, rng);
            stars.push(Star {
                x,
                y,
                px: x,
                py: y,
                layer,
                base: match layer {
                    0 => rng.range(0.25, 0.55) as f32,
                    1 => rng.range(0.45, 0.8) as f32,
                    _ => rng.range(0.65, 1.0) as f32,
                },
                phase: rng.range(0.0, std::f64::consts::TAU),
                twinkle: rng.range(0.6, 2.4),
                colour: if rng.f64() < 0.18 {
                    usize::MAX
                } else {
                    rng.below(3)
                },
            });
        }
        Starfield {
            stars,
            dw,
            dh,
            cfg,
            hole,
            age: 0.0,
            dt: 1.0 / fps.max(1.0),
            meteor: None,
            meteor_in: rng.range(3.0, 8.0),
        }
    }

    /// Twinkle factor for a star at the current age, in [0, 1].
    fn twinkle_at(&self, s: &Star) -> f32 {
        (0.5 + 0.5 * (self.age * s.twinkle + s.phase).sin()) as f32
    }

    fn star_colour(&self, s: &Star, pal: &Palette) -> Rgb {
        if s.colour == usize::MAX {
            pal.logo_rest
        } else {
            pal.bodies[s.colour % 3].trail
        }
    }

    /// Outward velocity for a star: radial direction from the centre, with a
    /// hyperdrive term that accelerates stars the further out they are, times
    /// the per-layer factor and the configured speed. A free function of the
    /// dimensions (rather than `&self`) so [`Starfield::advance`] can call it
    /// while iterating mutably.
    ///
    /// With a hole, two things change: stars just outside the rim pick up a
    /// tangential swirl (orbital motion around the hole), and anything that
    /// would cross the rim is deflected around it instead of through it.
    fn velocity_at(
        dw: f64,
        dh: f64,
        cfg: StarCfg,
        hole: Option<&GatherCtx>,
        s: &Star,
    ) -> (f64, f64) {
        let (cx, cy) = (dw * 0.5, dh * 0.5);
        let (dx, dy) = (s.x - cx, s.y - cy);
        let r = (dx * dx + dy * dy).sqrt();
        let max_r = (dw * dw + dh * dh).sqrt() * 0.5;
        let (mut ux, mut uy) = if r > 1e-6 { (dx / r, dy / r) } else { (1.0, 0.0) };
        // Base drift so the centre is never dead still, plus a quadratic warp
        // gain with distance: near-centre stars crawl while edge stars streak.
        // Quadratic (not linear) is what sells hyperdrive -- most of the sky
        // stays readable and only the rim really flies.
        let layer_f = match s.layer {
            0 => 0.35,
            1 => 0.7,
            _ => 1.0,
        };
        let base = (dh / 60.0).clamp(3.0, 18.0) * cfg.speed * layer_f;
        let t = (r / max_r.max(1.0)).clamp(0.0, 1.0);
        let mut v = (base * 0.3 + base * cfg.warp * 6.0 * t * t).min(1200.0);

        if let Some(h) = hole {
            let d = h.norm(s.x, s.y);
            let edge = h.hole_edge();
            // Swirl zone in rim units: the rim plus a margin that grows with
            // speed, so fast stars start turning before the hole fills their
            // windscreen. 60 rim radii per 1000 dots/s is ~one frame of rim-
            // speed travel at 60fps, doubled for comfort.
            let zone = edge + (rim_speed(dh, cfg) / 60.0) * 0.12 / h.hole_rx.max(1.0) * edge;
            if d < zone * 3.0 {
                // Orbital swirl near the hole: blend the radial direction into
                // a tangent, strongest at the rim, fading across the zone.
                // d is normalised so 1.0 is the ring's outer edge and `edge`
                // is the rim: measure the falloff in units of rim distance.
                let over = ((d - edge) / (zone * 3.0 - edge).max(1e-6)).clamp(0.0, 1.0);
                let swirl = (1.0 - over).powf(1.5);
                let (tx, ty) = (-uy, ux);
                ux = ux * (1.0 - swirl) + tx * swirl;
                uy = uy * (1.0 - swirl) + ty * swirl;
                // Slingshot: the rim itself is the fastest water in the sky.
                v += base * 2.0 * swirl;
                if d <= edge * 1.02 {
                    // On the rim: push along the tangent only, never inward.
                    // Anything the tangent still carries inside is caught by
                    // the occlusion in draw() and the respawn in advance().
                    v = v.max(base * 2.5);
                }
            }
        }
        (ux * v, uy * v)
    }
}

/// Rim speed of a near-layer star: the fastest water in the sky, and the
/// right scale for spawn clearance and the swirl zone. Mirrors the formula in
/// [`Starfield::velocity_at`] (t = 1 at the rim) without needing a star.
fn rim_speed(dh: f64, cfg: StarCfg) -> f64 {
    let base = (dh / 60.0).clamp(3.0, 18.0) * cfg.speed;
    base * 0.3 + base * cfg.warp * 6.0
}

/// Respawn a recycled star into the mid-field: a ring around the centre that
/// stays on-canvas even on small or narrow canvases. The hole's surroundings
/// are where the action is, and the swirl carries returnees around it -- edge
/// respawns would take seconds to fly back at low speed and read as the sky
/// draining.
fn respawn_midfield(
    dw: f64,
    dh: f64,
    hole: Option<&GatherCtx>,
    clearance: f64,
    rng: &mut Rng,
) -> (f64, f64) {
    // Ring radius fits inside the canvas with a small margin...
    let max_rr = (dw.min(dh) * 0.5 - 8.0).max(8.0);
    for _ in 0..16 {
        let rr = max_rr * rng.range(0.55, 1.0);
        let th = rng.range(0.0, std::f64::consts::TAU);
        let (x, y) = (dw * 0.5 + rr * th.cos(), dh * 0.5 + rr * th.sin());
        if x < 0.0 || y < 0.0 || x > dw || y > dh {
            continue;
        }
        if hole.is_some_and(|h| h.occludes(x, y)) {
            continue;
        }
        return (x, y);
    }
    spawn_point(dw, dh, hole, clearance, rng)
}

/// Area-scaled star count: enough to feel like a sky at any resolution,
/// clamped so a small logo still has sources and a huge canvas stays cheap.
fn default_count(dw: f64, dh: f64) -> usize {
    ((dw * dh / 900.0) as usize).clamp(400, 4000)
}

/// A spawn point outside the black hole (or anywhere, with no hole).
/// The exclusion zone is the rim puffed out by a speed-scaled margin: faster
/// stars cover more ground per frame, so they need more clearance to avoid
/// visibly spawning on top of the hole. Rejection-sampled; the hole covers a
/// small fraction of the screen, so this terminates immediately in practice.
fn spawn_point(
    dw: f64,
    dh: f64,
    hole: Option<&GatherCtx>,
    speed_dots_per_frame: f64,
    rng: &mut Rng,
) -> (f64, f64) {
    // Margin for one frame of travel plus the star's own radius, so nothing
    // pops into existence touching the rim.
    let margin = speed_dots_per_frame + 3.0;
    for _ in 0..16 {
        let (x, y) = (rng.range(0.0, dw), rng.range(0.0, dh));
        let clear = match hole {
            None => true,
            Some(h) => {
                let ex = (x - h.cx) / (h.hole_rx + margin).max(1.0);
                let ey = (y - h.cy) / (h.hole_ry + margin).max(1.0);
                ex * ex + ey * ey > 1.0
            }
        };
        if clear {
            return (x, y);
        }
    }
    // Pathological hole (huge `--hole-size`): fall back to a corner, which the
    // swirl below will carry outward anyway.
    (0.0, 0.0)
}

/// Far stars outnumber near ones, as in a real sky.
fn weighted_layer(rng: &mut Rng) -> usize {
    let r = rng.f64();
    if r < 0.5 {
        0
    } else if r < 0.85 {
        1
    } else {
        2
    }
}

impl GatherPhase for Starfield {
    fn advance(&mut self) {
        self.age += self.dt;
        // Copy out what velocity() needs: it borrows self while stars are
        // mutably borrowed below.
        let (dw, dh, cfg, hole, age, dt) =
            (self.dw, self.dh, self.cfg, self.hole, self.age, self.dt);
        for s in &mut self.stars {
            s.px = s.x;
            s.py = s.y;
            // Substep fast stars so no single step tunnels past the hole rim
            // or the screen edge: split the frame into steps of at most ~2
            // dots. Cheap (usually 1 step) and keeps every invariant exact.
            // The velocity is recomputed each substep so the swirl bends the
            // path instead of firing straight through the hole.
            let step_len = {
                let (vx, vy) = Self::velocity_at(dw, dh, cfg, hole.as_ref(), s);
                (vx * vx + vy * vy).sqrt() * dt
            };
            let steps = ((step_len / 2.0).ceil() as usize).clamp(1, 32);
            for _ in 0..steps {
                let (vx, vy) = Self::velocity_at(dw, dh, cfg, hole.as_ref(), s);
                s.x += vx * dt / steps as f64;
                s.y += vy * dt / steps as f64;
                if hole.is_some_and(|h| h.occludes(s.x, s.y)) {
                    break;
                }
                if s.x < -4.0 || s.y < -4.0 || s.x > dw + 4.0 || s.y > dh + 4.0 {
                    break;
                }
            }
            if hole.is_some_and(|h| h.occludes(s.x, s.y)) {
                // Swallowed by the hole: respawn upfield of the swirl, not at
                // the screen edge -- edge respawns take seconds to fly back at
                // low speed and read as the sky draining. Fall back to a
                // scratch RNG so advance() needs no &mut rng.
                let mut dummy = Rng::new(
                    (s.x.to_bits() ^ s.y.to_bits().wrapping_mul(31))
                        .wrapping_add((age * 1e6) as u64)
                        | 1,
                );
                (s.x, s.y) = respawn_midfield(
                    dw,
                    dh,
                    hole.as_ref(),
                    rim_speed(dh, cfg) * dt,
                    &mut dummy,
                );
                s.px = s.x;
                s.py = s.y;
                continue;
            }
            // Recycle past the edge back into the mid-field: same reasoning
            // as the hole respawn -- the sky must never visibly drain.
            if s.x < -4.0 || s.y < -4.0 || s.x > dw + 4.0 || s.y > dh + 4.0 {
                let mut dummy = Rng::new(
                    (s.x.to_bits() ^ s.y.to_bits().wrapping_mul(31))
                        .wrapping_add((age * 1e6) as u64)
                        | 1,
                );
                (s.x, s.y) = respawn_midfield(
                    dw,
                    dh,
                    hole.as_ref(),
                    rim_speed(dh, cfg) * dt,
                    &mut dummy,
                );
                s.px = s.x;
                s.py = s.y;
            }
        }

        // Meteors: at most one at a time, a few seconds apart.
        self.meteor_in -= self.dt;
        match &mut self.meteor {
            Some(m) => {
                m.x += m.vx * self.dt;
                m.y += m.vy * self.dt;
                m.life -= self.dt;
                // Consumed by the hole mid-flight: it flares on the rim
                // (drawn bright as life runs out) instead of crossing the dark.
                let swallowed =
                    self.hole.is_some_and(|h| h.occludes(m.x, m.y));
                if swallowed {
                    m.life = m.life.min(0.25);
                }
                if m.life <= 0.0
                    || m.x < -self.dw * 0.2
                    || m.y < -self.dh * 0.2
                    || m.x > self.dw * 1.2
                    || m.y > self.dh * 1.2
                {
                    self.meteor = None;
                    self.meteor_in = 4.0 + (self.age % 5.0);
                }
            }
            None => {
                if self.meteor_in <= 0.0 {
                    // Start near the top, streak down-left or down-right.
                    let speed = (self.dw.max(self.dh) * 0.9).clamp(120.0, 700.0);
                    let dir = if self.age % 2.0 < 1.0 { -0.6 } else { 0.6 };
                    self.meteor = Some(Meteor {
                        x: self.dw * 0.15 + (self.age % 1.0) * self.dw * 0.7,
                        y: self.dh * 0.05,
                        vx: speed * dir,
                        vy: speed * 0.45,
                        life: 1.1,
                        max_life: 1.1,
                    });
                }
            }
        }
    }

    fn draw(&self, canvas: &mut Canvas, pal: &Palette, _style: Style) {
        let radius = self.cfg.size;
        for s in &self.stars {
            // Occluded by the hole: the stage paints the hole behind us, so
            // skip rather than punching a star-coloured dot into the dark.
            if self.hole.is_some_and(|h| h.occludes(s.x, s.y)) {
                continue;
            }
            let tw = self.twinkle_at(s);
            let b = s.base * (0.35 + 0.65 * tw);
            let col = self.star_colour(s, pal);
            let colour =
                palette::mix(pal.background, col, (0.30 + 0.70 * b).clamp(0.0, 1.0));
            let (x0, y0) = (s.x.round() as i32, s.y.round() as i32);
            let (x1, y1) = (s.px.round() as i32, s.py.round() as i32);
            let stretch = ((x0 - x1).abs() + (y0 - y1).abs()) as f32;
            // Motion streak: the faster the star, the longer and brighter its
            // tail. Near-centre stars are near-static points; edge stars are
            // streaks. The disc gives near stars real size.
            let r = star_radius(s.layer, radius);
            let inten = 0.4 + b + stretch.min(12.0) * 0.12;
            if r > 0 {
                let disc = Canvas::disc(r);
                for &(ox, oy) in &disc {
                    canvas.dot(x0 + ox, y0 + oy, colour, 0.4 + b);
                }
            }
            if x0 == x1 && y0 == y1 && r > 0 {
                // Static and already stamped: nothing more to draw.
            } else {
                canvas.line(x1, y1, x0, y0, colour, inten, 0);
            }
            canvas.dot(x0, y0, colour, 2.0 + b * 4.0);
        }
        if let Some(m) = &self.meteor {
            // Consumed by the hole: a meteor that reaches the disc flares on
            // the rim instead of crossing the dark.
            if !self.hole.is_some_and(|h| h.occludes(m.x, m.y)) {
                let fade = (m.life / m.max_life).clamp(0.0, 1.0) as f32;
                let col = palette::mix(pal.background, pal.logo_rest, 0.35 + 0.65 * fade);
                let tail = 26.0 * fade as f64 + 6.0;
                let len = (m.vx * m.vx + m.vy * m.vy).sqrt().max(1.0);
                let (ux, uy) = (m.vx / len, m.vy / len);
                canvas.line(
                    m.x.round() as i32,
                    m.y.round() as i32,
                    (m.x - ux * tail).round() as i32,
                    (m.y - uy * tail).round() as i32,
                    col,
                    0.5 + fade,
                    0,
                );
                canvas.dot(m.x.round() as i32, m.y.round() as i32, col, 2.0 + 8.0 * fade);
            }
        }
    }

    fn source_points(&self, out: &mut Vec<(f64, f64, Rgb)>, pal: &Palette) {
        out.reserve(self.stars.len());
        for s in &self.stars {
            // Same occlusion as draw(): the morph targets the branding over
            // the hole, and sources from inside the disc would streak across
            // the dark.
            if self.hole.is_some_and(|h| h.occludes(s.x, s.y)) {
                continue;
            }
            out.push((s.x, s.y, self.star_colour(s, pal)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omarchy_screensaver_core::palette::Palette;

    fn field(seeds: u64, dw: f64, dh: f64, count: Option<usize>) -> Starfield {
        field_with_hole(seeds, dw, dh, count, None)
    }

    fn field_with_hole(
        seeds: u64,
        dw: f64,
        dh: f64,
        count: Option<usize>,
        hole: Option<GatherCtx>,
    ) -> Starfield {
        let mut rng = Rng::new(seeds);
        Starfield::new(dw, dh, &mut rng, count, 60.0, StarCfg::default(), hole)
    }

    fn test_hole(dw: f64, dh: f64) -> GatherCtx {
        // A hole like the stage builds for a centred art: deterministic and
        // big enough to matter (radius 0.3 of the smaller dimension).
        GatherCtx {
            cx: dw * 0.5,
            cy: dh * 0.5,
            hole_rx: dw.min(dh) * 0.3,
            hole_ry: dw.min(dh) * 0.15,
            ring_rx: dw.min(dh) * 0.3 + 4.0,
            ring_ry: dw.min(dh) * 0.15 + 2.0,
            ring_w: 4.0,
            dw,
            dh,
        }
    }

    #[test]
    fn count_scales_with_area_and_stays_bounded() {
        assert_eq!(default_count(266.0, 140.0), 400); // small canvas: floor
        assert!((1400..1600).contains(&default_count(1500.0, 900.0))); // mid-range scales
        assert_eq!(default_count(4000.0, 2000.0), 4000); // huge canvas: ceiling
        let f = field(7, 600.0, 336.0, Some(123));
        assert_eq!(f.stars.len(), 123, "explicit count must win");
    }

    #[test]
    fn twinkle_stays_in_range() {
        let mut f = field(3, 300.0, 160.0, Some(50));
        for _ in 0..600 {
            f.advance();
            for s in &f.stars {
                let tw = f.twinkle_at(s);
                assert!((0.0..=1.0).contains(&tw), "twinkle out of range: {tw}");
            }
        }
    }

    #[test]
    fn warp_recycles_stars_so_none_escape() {
        let mut f = field(11, 200.0, 100.0, Some(200));
        for _ in 0..3600 {
            f.advance();
            // Substepping keeps every star inside the margin every frame.
            for s in &f.stars {
                assert!(
                    (-4.0..204.0).contains(&s.x),
                    "star escaped in x: {}",
                    s.x
                );
                assert!(
                    (-4.0..104.0).contains(&s.y),
                    "star escaped in y: {}",
                    s.y
                );
            }
        }
    }

    #[test]
    fn warp_moves_stars_outward() {
        // No hole, one frame: nothing can recycle in a single step, so every
        // star must end further from the centre than it started. (Near-centre
        // stars move sub-dot distances but still outward -- strict > holds
        // because velocity is never zero.)
        let mut rng = Rng::new(21);
        let mut f = Starfield::new(400.0, 200.0, &mut rng, Some(400), 60.0, StarCfg::default(), None);
        let r0: Vec<f64> = f
            .stars
            .iter()
            .map(|s| ((s.x - 200.0).powi(2) + (s.y - 100.0).powi(2)).sqrt())
            .collect();
        f.advance();
        let mut grown = 0;
        for (s, r) in f.stars.iter().zip(&r0) {
            let r1 = ((s.x - 200.0).powi(2) + (s.y - 100.0).powi(2)).sqrt();
            if r1 > *r {
                grown += 1;
            }
        }
        assert_eq!(grown, 400, "warp did not push stars outward: only {grown}/400 grew");
    }

    #[test]
    fn calm_sky_barely_moves() {
        let mut rng = Rng::new(33);
        let mut f = Starfield::new(
            400.0,
            200.0,
            &mut rng,
            Some(100),
            60.0,
            StarCfg { size: 1.0, speed: 0.2, warp: 0.0 },
            None,
        );
        let before: Vec<(f64, f64)> = f.stars.iter().map(|s| (s.x, s.y)).collect();
        for _ in 0..60 {
            f.advance();
        }
        let moved: f64 = f
            .stars
            .iter()
            .zip(&before)
            .map(|(s, (x, y))| ((s.x - x).powi(2) + (s.y - y).powi(2)).sqrt())
            .sum::<f64>()
            / 100.0;
        assert!(moved < 3.0, "calm sky moved too far: {moved}");
    }

    #[test]
    fn star_radius_grows_with_size_and_layer() {
        assert_eq!(star_radius(0, 99.0), 0, "far stars stay single dots");
        assert_eq!(star_radius(1, 0.0), 0);
        assert!(star_radius(2, 1.2) > star_radius(1, 1.2));
        assert!(star_radius(1, 2.0) >= star_radius(1, 1.0));
    }

    #[test]
    fn sources_cover_the_canvas() {
        // No hole: every star is a source and they spread across quadrants.
        let f = field(5, 400.0, 200.0, Some(500));
        let pal = Palette::reference();
        let mut src = Vec::new();
        f.source_points(&mut src, &pal);
        assert_eq!(src.len(), 500);
        // Not all piled in one corner: spread across quadrants.
        let mut quads = [0; 4];
        for (x, y, _) in &src {
            quads[(*x >= 200.0) as usize + 2 * (*y >= 100.0) as usize] += 1;
        }
        assert!(quads.iter().all(|&q| q > 50), "stars are clumped: {quads:?}");
    }

    #[test]
    fn no_star_spawns_inside_the_hole() {
        let hole = test_hole(400.0, 200.0);
        let f = field_with_hole(5, 400.0, 200.0, Some(2000), Some(hole));
        let inside = f.stars.iter().filter(|s| hole.occludes(s.x, s.y)).count();
        assert_eq!(inside, 0, "{inside} stars spawned inside the hole");
    }

    #[test]
    fn the_hole_stays_clear_over_time() {
        let hole = test_hole(400.0, 200.0);
        let mut f = field_with_hole(5, 400.0, 200.0, Some(500), Some(hole));
        for _ in 0..600 {
            f.advance();
        }
        let inside = f.stars.iter().filter(|s| hole.occludes(s.x, s.y)).count();
        assert_eq!(inside, 0, "{inside} stars sitting inside the hole");
        // ...and the sky is still populated: nothing drained away.
        let pal = Palette::reference();
        let mut src = Vec::new();
        f.source_points(&mut src, &pal);
        assert!(
            src.len() >= 450,
            "occlusion ate too many sources: {}",
            src.len()
        );
    }

    #[test]
    fn stars_near_the_hole_move_tangentially() {
        // Just outside the rim the swirl dominates: velocity should be mostly
        // tangential, not radial.
        let hole = test_hole(400.0, 200.0);
        let s = Star {
            x: hole.cx + hole.hole_rx * 1.15,
            y: hole.cy,
            px: hole.cx + hole.hole_rx * 1.15,
            py: hole.cy,
            layer: 2,
            base: 1.0,
            phase: 0.0,
            twinkle: 1.0,
            colour: 0,
        };
        let (vx, vy) =
            Starfield::velocity_at(400.0, 200.0, StarCfg::default(), Some(&hole), &s);
        assert!(
            vy.abs() > vx.abs() * 2.0,
            "rim velocity not tangential: ({vx}, {vy})"
        );
    }

    #[test]
    fn far_stars_outnumber_near_ones() {
        let f = field(9, 400.0, 200.0, Some(2000));
        let (mut far, mut near) = (0, 0);
        for s in &f.stars {
            match s.layer {
                0 => far += 1,
                2 => near += 1,
                _ => {}
            }
        }
        assert!(far > near * 2, "far {far} vs near {near}");
    }
}
