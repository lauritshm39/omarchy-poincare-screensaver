//! Turning a cloud of trail dots into the logo (and back out again).
//!
//! Pairing two point clouds optimally is an assignment problem, and Hungarian
//! matching on a few thousand points per morph is far too slow for a frame
//! budget. Sorting both clouds by angle about their own centroid and zipping
//! them costs O(n log n), never crosses paths much, and -- because the
//! interpolation is done in polar coordinates -- reads as the orbit spiralling
//! inward and settling into the glyphs, which suits the subject.

use crate::canvas::Rgb;
use crate::rng::Rng;

pub struct Particle {
    r0: f64,
    th0: f64,
    r1: f64,
    dth: f64,
    pub colour: Rgb,
    delay: f32,
    /// Index into the art's cell list, or usize::MAX when this particle has no
    /// glyph to reveal (surplus particles that just fly in and fade).
    pub cell: usize,
}

pub struct Morph {
    pub particles: Vec<Particle>,
    cx: f64,
    cy: f64,
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Morph {
    /// `sources` and `targets` are in dot coordinates. `targets` carries the
    /// art cell index each point belongs to. The two are trimmed to a common
    /// length; whichever is longer keeps a uniformly spaced subset so the
    /// dropped points do not come from one region of the picture.
    pub fn new(
        cx: f64,
        cy: f64,
        mut sources: Vec<(f64, f64, Rgb)>,
        mut targets: Vec<(f64, f64, usize)>,
        rng: &mut Rng,
    ) -> Morph {
        let n = sources.len().min(targets.len());
        if n == 0 {
            return Morph { particles: Vec::new(), cx, cy };
        }
        thin_to(&mut sources, n);
        thin_to(&mut targets, n);

        let scen = centroid(sources.iter().map(|p| (p.0, p.1)));
        let tcen = centroid(targets.iter().map(|p| (p.0, p.1)));
        sort_by_polar(&mut sources, scen, |p| (p.0, p.1));
        sort_by_polar(&mut targets, tcen, |p| (p.0, p.1));

        let particles = sources
            .iter()
            .zip(targets.iter())
            .map(|(s, t)| {
                let (dx0, dy0) = (s.0 - cx, s.1 - cy);
                let (dx1, dy1) = (t.0 - cx, t.1 - cy);
                let th0 = dy0.atan2(dx0);
                let th1 = dy1.atan2(dx1);
                // Shortest way round, so nothing takes the long way about.
                let mut dth = (th1 - th0) % std::f64::consts::TAU;
                if dth > std::f64::consts::PI {
                    dth -= std::f64::consts::TAU;
                } else if dth < -std::f64::consts::PI {
                    dth += std::f64::consts::TAU;
                }
                Particle {
                    r0: (dx0 * dx0 + dy0 * dy0).sqrt(),
                    th0,
                    r1: (dx1 * dx1 + dy1 * dy1).sqrt(),
                    dth,
                    colour: s.2,
                    // Staggering arrival keeps the logo from snapping into
                    // existence all at once.
                    delay: rng.range(0.0, 0.30) as f32,
                    cell: t.2,
                }
            })
            .collect();

        Morph { particles, cx, cy }
    }

    /// Position and per-particle progress at overall progress `t` in [0, 1].
    pub fn at(&self, p: &Particle, t: f32) -> (f64, f64, f32) {
        let local = smoothstep((t - p.delay) / (1.0 - p.delay).max(1e-3));
        let e = local as f64;
        let r = p.r0 + (p.r1 - p.r0) * e;
        let th = p.th0 + p.dth * e;
        (self.cx + r * th.cos(), self.cy + r * th.sin(), local)
    }
}

fn centroid<I: Iterator<Item = (f64, f64)>>(it: I) -> (f64, f64) {
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
    for (x, y) in it {
        sx += x;
        sy += y;
        n += 1.0;
    }
    if n == 0.0 {
        (0.0, 0.0)
    } else {
        (sx / n, sy / n)
    }
}

fn sort_by_polar<T, F: Fn(&T) -> (f64, f64)>(v: &mut [T], c: (f64, f64), get: F) {
    v.sort_by(|a, b| {
        let key = |p: &T| {
            let (x, y) = get(p);
            let (dx, dy) = (x - c.0, y - c.1);
            (dy.atan2(dx), dx * dx + dy * dy)
        };
        let (ka, kb) = (key(a), key(b));
        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Keep `n` evenly spaced elements.
fn thin_to<T: Clone>(v: &mut Vec<T>, n: usize) {
    if v.len() <= n {
        return;
    }
    let src = std::mem::take(v);
    let len = src.len();
    *v = (0..n).map(|i| src[i * len / n].clone()).collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud(n: usize, r: f64) -> Vec<(f64, f64, Rgb)> {
        (0..n)
            .map(|i| {
                let t = std::f64::consts::TAU * i as f64 / n as f64;
                (100.0 + r * t.cos(), 100.0 + r * t.sin(), [1.0, 1.0, 1.0])
            })
            .collect()
    }

    fn targets(n: usize) -> Vec<(f64, f64, usize)> {
        (0..n).map(|i| (100.0 + i as f64, 100.0, i)).collect()
    }

    #[test]
    fn thin_to_keeps_a_spread_not_a_prefix() {
        let mut v: Vec<usize> = (0..100).collect();
        thin_to(&mut v, 10);
        assert_eq!(v.len(), 10);
        assert_eq!(v[0], 0);
        assert!(*v.last().unwrap() > 80, "thinning kept only the front: {v:?}");
    }

    #[test]
    fn thin_to_is_a_no_op_when_already_short_enough() {
        let mut v: Vec<usize> = (0..5).collect();
        thin_to(&mut v, 10);
        assert_eq!(v, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn mismatched_cloud_sizes_pair_up_completely() {
        let mut rng = Rng::new(1);
        for (s, t) in [(500, 20), (20, 500), (7, 7)] {
            let m = Morph::new(100.0, 100.0, cloud(s, 30.0), targets(t), &mut rng);
            assert_eq!(m.particles.len(), s.min(t), "{s} sources / {t} targets");
        }
    }

    #[test]
    fn an_empty_cloud_produces_no_particles() {
        let mut rng = Rng::new(1);
        assert!(Morph::new(0.0, 0.0, vec![], targets(5), &mut rng).particles.is_empty());
        assert!(Morph::new(0.0, 0.0, cloud(5, 1.0), vec![], &mut rng).particles.is_empty());
    }

    /// Every particle must start where it started and finish on its target,
    /// or glyphs would be left unclaimed or the logo would assemble crooked.
    #[test]
    fn particles_start_at_the_source_and_land_on_the_target() {
        let mut rng = Rng::new(7);
        let src = cloud(64, 40.0);
        let tgt = targets(64);
        let m = Morph::new(100.0, 100.0, src.clone(), tgt.clone(), &mut rng);

        for p in &m.particles {
            let (x0, y0, prog0) = m.at(p, 0.0);
            assert!(prog0 <= 0.001, "progress at t=0 was {prog0}");
            assert!(
                src.iter().any(|s| (s.0 - x0).hypot(s.1 - y0) < 1e-6),
                "start ({x0}, {y0}) is not one of the sources"
            );
            let (x1, y1, prog1) = m.at(p, 1.0);
            assert!((prog1 - 1.0).abs() < 1e-6, "progress at t=1 was {prog1}");
            assert!(
                tgt.iter().any(|t| (t.0 - x1).hypot(t.1 - y1) < 1e-6),
                "end ({x1}, {y1}) is not one of the targets"
            );
        }
    }

    #[test]
    fn every_target_cell_is_claimed_when_the_counts_match() {
        let mut rng = Rng::new(3);
        let m = Morph::new(100.0, 100.0, cloud(40, 25.0), targets(40), &mut rng);
        let mut cells: Vec<usize> = m.particles.iter().map(|p| p.cell).collect();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(cells.len(), 40, "some glyph cells would never be revealed");
    }

    #[test]
    fn motion_is_monotonic_and_stays_in_bounds() {
        let mut rng = Rng::new(11);
        let m = Morph::new(100.0, 100.0, cloud(16, 30.0), targets(16), &mut rng);
        for p in &m.particles {
            let mut last = -1.0f32;
            for i in 0..=20 {
                let (x, y, prog) = m.at(p, i as f32 / 20.0);
                assert!(prog >= last - 1e-6, "progress went backwards");
                assert!(prog >= 0.0 && prog <= 1.0);
                assert!(x.is_finite() && y.is_finite());
                last = prog;
            }
        }
    }

    #[test]
    fn smoothstep_is_clamped_and_symmetric() {
        assert_eq!(smoothstep(-5.0), 0.0);
        assert_eq!(smoothstep(0.0), 0.0);
        assert_eq!(smoothstep(1.0), 1.0);
        assert_eq!(smoothstep(9.0), 1.0);
        assert!((smoothstep(0.5) - 0.5).abs() < 1e-6);
    }
}
