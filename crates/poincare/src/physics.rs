//! Planar Newtonian three-body problem, integrated with a 4th-order symplectic
//! scheme. Units are G = 1; masses are per-orbit.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Vec2 { x, y }
    }
    pub fn norm(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl std::ops::Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
}
impl std::ops::AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        self.x += o.x;
        self.y += o.y;
    }
}

#[derive(Clone, Debug)]
pub struct System {
    pub pos: [Vec2; 3],
    pub vel: [Vec2; 3],
    pub mass: [f64; 3],
    /// Plummer softening. A screensaver must survive the near-collisions that
    /// end most three-body integrations; this bounds the acceleration without
    /// visibly perturbing the well-separated periodic orbits we ship.
    pub soft2: f64,
}

impl System {
    pub fn new(pos: [Vec2; 3], vel: [Vec2; 3], mass: [f64; 3]) -> Self {
        System { pos, vel, mass, soft2: 1e-8 }
    }

    fn accel(&self) -> [Vec2; 3] {
        let mut a = [Vec2::default(); 3];
        for i in 0..3 {
            for j in (i + 1)..3 {
                let d = self.pos[j] - self.pos[i];
                let r2 = d.x * d.x + d.y * d.y + self.soft2;
                let inv_r3 = 1.0 / (r2 * r2.sqrt());
                a[i] += d * (self.mass[j] * inv_r3);
                a[j] += d * (-self.mass[i] * inv_r3);
            }
        }
        a
    }

    /// One Yoshida 4th-order symplectic step. Energy error stays bounded over
    /// the hours a screensaver may run, which a Runge-Kutta scheme would not.
    pub fn step(&mut self, dt: f64) {
        const CBRT2: f64 = 1.259_921_049_894_873_2; // 2^(1/3)
        let w1 = 1.0 / (2.0 - CBRT2);
        let w0 = -CBRT2 * w1;
        let c = [w1 * 0.5, (w0 + w1) * 0.5, (w0 + w1) * 0.5, w1 * 0.5];
        let d = [w1, w0, w1];

        for k in 0..3 {
            for i in 0..3 {
                self.pos[i] += self.vel[i] * (c[k] * dt);
            }
            let a = self.accel();
            for i in 0..3 {
                self.vel[i] += a[i] * (d[k] * dt);
            }
        }
        for i in 0..3 {
            self.pos[i] += self.vel[i] * (c[3] * dt);
        }
    }

    pub fn energy(&self) -> f64 {
        let mut e = 0.0;
        for i in 0..3 {
            let v = self.vel[i];
            e += 0.5 * self.mass[i] * (v.x * v.x + v.y * v.y);
        }
        for i in 0..3 {
            for j in (i + 1)..3 {
                // The softened potential whose gradient is the softened force
                // used in accel(). Using the bare 1/r here instead would
                // describe a different Hamiltonian from the one being
                // integrated, and energy would appear to jump whenever the
                // softening actually mattered.
                let d = self.pos[j] - self.pos[i];
                let r = (d.x * d.x + d.y * d.y + self.soft2).sqrt();
                e -= self.mass[i] * self.mass[j] / r;
            }
        }
        e
    }

    /// Separation of the closest pair.
    pub fn min_separation(&self) -> f64 {
        let mut m = f64::MAX;
        for i in 0..3 {
            for j in (i + 1)..3 {
                m = m.min((self.pos[j] - self.pos[i]).norm());
            }
        }
        m
    }

    /// A step that resolves the tightest encounter currently in progress.
    ///
    /// A fixed step sized for the leisurely part of an orbit walks straight
    /// through a close approach and injects energy, which is what ejects a body
    /// and destroys the picture. The local dynamical time goes as r^(3/2), so
    /// scaling the step that way costs nothing while the bodies are apart and
    /// automatically buys resolution when they are not. Separations here are of
    /// order one, so r = 1 gives the full step.
    pub fn suggested_dt(&self, max_dt: f64, min_dt: f64) -> f64 {
        let r = self.min_separation();
        if !r.is_finite() {
            return min_dt;
        }
        (max_dt * r.powf(1.5)).clamp(min_dt, max_dt)
    }

    pub fn centre_of_mass(&self) -> Vec2 {
        let m: f64 = self.mass.iter().sum();
        let mut c = Vec2::default();
        for i in 0..3 {
            c += self.pos[i] * self.mass[i];
        }
        c * (1.0 / m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two equal masses in a circular orbit about their centre of mass. The
    /// period follows from Newton alone, so this checks the integrator against
    /// arithmetic rather than against itself.
    #[test]
    fn circular_two_body_returns_to_its_start() {
        // Bodies at +/-r with the third far enough away to be negligible is not
        // possible in a 3-body type, so use a third body at the centre of mass
        // with zero mass instead.
        let r = 1.0;
        // For two masses m at separation 2r: v = sqrt(G m / (4 r)).
        let v = (1.0f64 / (4.0 * r)).sqrt();
        let mut s = System::new(
            [Vec2::new(-r, 0.0), Vec2::new(r, 0.0), Vec2::new(0.0, 0.0)],
            [Vec2::new(0.0, -v), Vec2::new(0.0, v), Vec2::new(0.0, 0.0)],
            [1.0, 1.0, 0.0],
        );
        let period = std::f64::consts::TAU * r / v;
        let steps = 200_000;
        for _ in 0..steps {
            s.step(period / steps as f64);
        }
        assert!(
            (s.pos[0] - Vec2::new(-r, 0.0)).norm() < 1e-6,
            "body 0 drifted to {:?}",
            s.pos[0]
        );
    }

    #[test]
    fn energy_is_conserved_over_a_long_run() {
        let mut s = System::new(
            [Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 0.0)],
            [Vec2::new(0.3068, 0.1255), Vec2::new(0.3068, 0.1255), Vec2::new(-0.6136, -0.2510)],
            [1.0, 1.0, 1.0],
        );
        let e0 = s.energy();
        for _ in 0..500_000 {
            let dt = s.suggested_dt(2.0e-4, 1.0e-8);
            s.step(dt);
        }
        let drift = ((s.energy() - e0) / e0.abs()).abs();
        assert!(drift < 1e-6, "energy drifted by {drift:e}");
    }

    #[test]
    fn centre_of_mass_does_not_move_when_it_starts_at_rest() {
        let mut s = System::new(
            [Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 0.0)],
            [Vec2::new(0.4, 0.2), Vec2::new(0.4, 0.2), Vec2::new(-0.8, -0.4)],
            [1.0, 1.0, 1.0],
        );
        for _ in 0..50_000 {
            s.step(1e-4);
        }
        assert!(s.centre_of_mass().norm() < 1e-9, "com moved to {:?}", s.centre_of_mass());
    }

    /// The whole reason the stepper is adaptive: a fixed step walks through a
    /// close approach and injects energy.
    #[test]
    fn suggested_dt_shrinks_during_a_close_approach() {
        let far = System::new(
            [Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 2.0)],
            [Vec2::default(); 3],
            [1.0, 1.0, 1.0],
        );
        let near = System::new(
            [Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.01, 0.0)],
            [Vec2::default(); 3],
            [1.0, 1.0, 1.0],
        );
        let (max_dt, min_dt) = (2.0e-4, 1.0e-8);
        assert_eq!(far.suggested_dt(max_dt, min_dt), max_dt);
        assert!(near.suggested_dt(max_dt, min_dt) < max_dt / 100.0);
    }

    #[test]
    fn suggested_dt_is_bounded_even_at_contact() {
        let touching = System::new(
            [Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)],
            [Vec2::default(); 3],
            [1.0, 1.0, 1.0],
        );
        let dt = touching.suggested_dt(2.0e-4, 1.0e-8);
        assert!(dt >= 1.0e-8 && dt <= 2.0e-4, "dt out of bounds: {dt}");
    }
}
