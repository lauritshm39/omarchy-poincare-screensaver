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
use omarchy_screensaver_core::stage::{GatherPhase, Style};

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
        StarCfg { size: 1.2, speed: 2.0, warp: 1.0 }
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
    /// Seconds since creation, advanced once per frame.
    age: f64,
    dt: f64,
    meteor: Option<Meteor>,
    /// Seconds until the next meteor may spawn.
    meteor_in: f64,
}

impl Starfield {
    /// `count` overrides the area-scaled default. `fps` sets the per-frame
    /// time step driving drift and twinkle.
    pub fn new(
        dw: f64,
        dh: f64,
        rng: &mut Rng,
        count: Option<usize>,
        fps: f64,
        cfg: StarCfg,
    ) -> Starfield {
        let n = count.unwrap_or_else(|| default_count(dw, dh));
        let mut stars = Vec::with_capacity(n);
        for _ in 0..n {
            let layer = weighted_layer(rng);
            let (x, y) = (rng.range(0.0, dw), rng.range(0.0, dh));
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
    fn velocity_at(dw: f64, dh: f64, cfg: StarCfg, s: &Star) -> (f64, f64) {
        let (cx, cy) = (dw * 0.5, dh * 0.5);
        let (dx, dy) = (s.x - cx, s.y - cy);
        let r = (dx * dx + dy * dy).sqrt();
        let max_r = (dw * dw + dh * dh).sqrt() * 0.5;
        let (ux, uy) = if r > 1e-6 { (dx / r, dy / r) } else { (1.0, 0.0) };
        // Base drift so the centre is never dead still, plus warp gain with
        // distance: near-centre stars crawl, edge stars streak.
        let base = (dh / 60.0).clamp(3.0, 18.0)
            * cfg.speed
            * match s.layer {
                0 => 0.35,
                1 => 0.7,
                _ => 1.0,
            };
        let gain = base * cfg.warp * (r / max_r.max(1.0));
        let v = base * 0.3 + gain;
        (ux * v, uy * v)
    }
}

/// Area-scaled star count: enough to feel like a sky at any resolution,
/// clamped so a small logo still has sources and a huge canvas stays cheap.
fn default_count(dw: f64, dh: f64) -> usize {
    ((dw * dh / 900.0) as usize).clamp(400, 4000)
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
        let (dw, dh, cfg, age, dt) = (self.dw, self.dh, self.cfg, self.age, self.dt);
        for s in &mut self.stars {
            s.px = s.x;
            s.py = s.y;
            let (vx, vy) = Self::velocity_at(dw, dh, cfg, s);
            s.x += vx * dt;
            s.y += vy * dt;
            // Recycle past the edge back near the centre: warp is a fountain,
            // not a drain. Jitter the respawn so returnees do not line up.
            if s.x < -4.0 || s.y < -4.0 || s.x > dw + 4.0 || s.y > dh + 4.0 {
                let (cx, cy) = (dw * 0.5, dh * 0.5);
                let mut dummy = Rng::new(
                    (s.x.to_bits() ^ s.y.to_bits().wrapping_mul(31))
                        .wrapping_add((age * 1e6) as u64)
                        | 1,
                );
                let th = dummy.range(0.0, std::f64::consts::TAU);
                let rr = dummy.range(0.0, dw.min(dh) * 0.06);
                s.x = cx + rr * th.cos();
                s.y = cy + rr * th.sin();
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

    fn source_points(&self, out: &mut Vec<(f64, f64, Rgb)>, pal: &Palette) {
        out.reserve(self.stars.len());
        for s in &self.stars {
            out.push((s.x, s.y, self.star_colour(s, pal)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omarchy_screensaver_core::palette::Palette;

    fn field(seeds: u64, dw: f64, dh: f64, count: Option<usize>) -> Starfield {
        let mut rng = Rng::new(seeds);
        Starfield::new(dw, dh, &mut rng, count, 60.0, StarCfg::default())
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
        }
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

    #[test]
    fn warp_moves_stars_outward() {
        // With warp at full and twinkle irrelevant, the mean radius must grow.
        // Measured over 45 frames: long enough to move, short enough that no
        // star has recycled back to the centre yet (which would drag the mean
        // back down and make this assert on the wrong thing).
        let mut f = field(21, 400.0, 200.0, Some(400));
        let mean_r = |f: &Starfield| {
            f.stars
                .iter()
                .map(|s| ((s.x - 200.0).powi(2) + (s.y - 100.0).powi(2)).sqrt())
                .sum::<f64>()
                / 400.0
        };
        let r0 = mean_r(&f);
        for _ in 0..45 {
            f.advance();
        }
        let r1 = mean_r(&f);
        assert!(r1 > r0, "warp did not push stars outward: {r0} -> {r1}");
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
        let f = field(5, 400.0, 200.0, Some(500));
        let pal = Palette::reference();
        let mut src = Vec::new();
        f.source_points(&mut src, &pal);
        assert_eq!(src.len(), 500);
        assert!(src.iter().all(|(x, y, _)| (0.0..400.0).contains(x) && (0.0..200.0).contains(y)));
        // Not all piled in one corner: spread across quadrants.
        let mut quads = [0; 4];
        for (x, y, _) in &src {
            quads[(*x >= 200.0) as usize + 2 * (*y >= 100.0) as usize] += 1;
        }
        assert!(quads.iter().all(|&q| q > 50), "stars are clumped: {quads:?}");
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
