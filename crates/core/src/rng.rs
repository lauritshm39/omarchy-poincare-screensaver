//! Tiny xorshift PRNG. Keeps the dependency list at exactly one crate.

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Any non-zero state works; splitmix the seed so nearby seeds diverge.
        let mut s = seed.wrapping_add(0x9E3779B97F4A7C15);
        s = (s ^ (s >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        s = (s ^ (s >> 27)).wrapping_mul(0x94D049BB133111EB);
        Rng((s ^ (s >> 31)) | 1)
    }

    pub fn from_clock() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5DEECE66D);
        Rng::new(n ^ (std::process::id() as u64) << 32)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform in [0, 1).
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in [lo, hi).
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.f64() * (hi - lo)
    }

    /// Uniform in [0, n).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next_u64() % n as u64) as usize }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_stay_in_range() {
        let mut r = Rng::new(42);
        for _ in 0..10_000 {
            let f = r.f64();
            assert!((0.0..1.0).contains(&f), "f64() returned {f}");
            let v = r.range(-3.0, 7.0);
            assert!((-3.0..7.0).contains(&v), "range() returned {v}");
            assert!(r.below(5) < 5);
        }
        assert_eq!(r.below(0), 0, "below(0) must not divide by zero");
    }

    #[test]
    fn the_same_seed_gives_the_same_stream_and_different_seeds_diverge() {
        let take = |seed| {
            let mut r = Rng::new(seed);
            (0..8).map(|_| r.next_u64()).collect::<Vec<_>>()
        };
        assert_eq!(take(7), take(7));
        assert_ne!(take(7), take(8), "adjacent seeds must not produce the same stream");
    }

    #[test]
    fn output_is_reasonably_spread() {
        let mut r = Rng::new(99);
        let mut buckets = [0usize; 10];
        for _ in 0..100_000 {
            buckets[(r.f64() * 10.0) as usize % 10] += 1;
        }
        for (i, n) in buckets.iter().enumerate() {
            assert!((8_000..12_000).contains(n), "bucket {i} had {n} of 100000");
        }
    }
}
