//! A twinkling starfield that doubles as gather-phase sources.
//!
//! Stars drift slowly on two or three parallax layers, each twinkling at its
//! own rate and phase. When the stage collects [`Starfield::source_points`],
//! the current star positions become the morph sources, so the night sky
//! gathers into the branding art through the shared [`Morph`][crate::morph::Morph].

use omarchy_screensaver_core::canvas::{Canvas, Rgb};
use omarchy_screensaver_core::palette::{self, Palette};
use omarchy_screensaver_core::rng::Rng;
use omarchy_screensaver_core::stage::{GatherPhase, Style};

/// One star, in dot coordinates.
struct Star {
    x: f64,
    y: f64,
    /// Parallax layer: 0 is far (slow, dim), 2 is near (faster, brighter).
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

pub struct Starfield {
    stars: Vec<Star>,
    dw: f64,
    dh: f64,
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
    pub fn new(dw: f64, dh: f64, rng: &mut Rng, count: Option<usize>, fps: f64) -> Starfield {
        let n = count.unwrap_or_else(|| default_count(dw, dh));
        let mut stars = Vec::with_capacity(n);
        for _ in 0..n {
            let layer = weighted_layer(rng);
            stars.push(Star {
                x: rng.range(0.0, dw),
                y: rng.range(0.0, dh),
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

    /// Drift per second for a layer, in dots. Slow on purpose: the sky should
    /// feel still, with just enough motion to read as alive.
    fn drift(layer: usize, dh: f64) -> (f64, f64) {
        let base = (dh / 140.0).clamp(1.5, 8.0);
        match layer {
            0 => (base * 0.25, base * 0.12),
            1 => (base * 0.6, base * 0.3),
            _ => (base, base * 0.5),
        }
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
        for s in &mut self.stars {
            let (dx, dy) = Starfield::drift(s.layer, self.dh);
            s.x += dx * self.dt;
            s.y += dy * self.dt;
            // Wrap around the edges so the density stays constant.
            if s.x >= self.dw {
                s.x -= self.dw;
            }
            if s.y >= self.dh {
                s.y -= self.dh;
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
        for s in &self.stars {
            let tw = self.twinkle_at(s);
            let b = s.base * (0.35 + 0.65 * tw);
            let col = self.star_colour(s, pal);
            let colour =
                palette::mix(pal.background, col, (0.30 + 0.70 * b).clamp(0.0, 1.0));
            canvas.dot(s.x.round() as i32, s.y.round() as i32, colour, 0.4 + b);
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
        Starfield::new(dw, dh, &mut rng, count, 60.0)
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
    fn drift_wraps_so_stars_never_leave() {
        let mut f = field(11, 200.0, 100.0, Some(200));
        for _ in 0..3600 {
            f.advance();
        }
        for s in &f.stars {
            assert!((0.0..200.0).contains(&s.x), "star escaped in x: {}", s.x);
            assert!((0.0..100.0).contains(&s.y), "star escaped in y: {}", s.y);
        }
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
