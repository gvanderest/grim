//! Deterministic RNG for game systems (combat rolls, flee odds, loot).
//!
//! [`Xorshift`] is a xorshift64* generator: 10 lines, no dependencies,
//! seedable for deterministic tests. Game code holds one in a resource
//! (`CombatRng`); tests seed it directly.

/// Xorshift64* generator. Seed must be nonzero (zero maps to a fixed seed).
#[derive(Clone, Copy, Debug)]
pub struct Xorshift(pub u64);

impl Xorshift {
    /// Build from a seed. A zero seed maps to a fixed nonzero constant so a
    /// default-constructed generator still advances.
    pub fn seed(seed: u64) -> Self {
        Self(if seed == 0 { 0x853c49e6748fea9b } else { seed })
    }

    /// Next raw value.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545f4914f6cdd1d)
    }

    /// Uniform value in `0..n`. Panics on `n == 0` — callers pass a literal
    /// count, never player input.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// True with probability `pct` percent. `pct >= 100` always hits.
    pub fn chance(&mut self, pct: u32) -> bool {
        if pct >= 100 {
            return true;
        }
        self.below(100) < u64::from(pct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Xorshift::seed(42);
        let mut b = Xorshift::seed(42);
        for _ in 0..16 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn below_stays_in_range() {
        let mut rng = Xorshift::seed(7);
        for _ in 0..200 {
            assert!(rng.below(10) < 10);
        }
    }

    #[test]
    fn chance_edges() {
        let mut rng = Xorshift::seed(1);
        assert!(rng.chance(100));
        assert!(rng.chance(200));
        assert!(!rng.chance(0));
    }

    #[test]
    fn zero_seed_still_advances() {
        let mut rng = Xorshift::seed(0);
        assert_ne!(rng.next_u64(), rng.next_u64());
    }
}
