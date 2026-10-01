//! Pure combat math: hit odds, damage, awards. No Bevy, no RNG resource —
//! every roll takes an explicit `&mut Xorshift` so tests seed deterministically.
//! Formula inputs are levels only (the #60 Stats seam swaps inputs later
//! without touching the loop).

use grim_core::Xorshift;

/// Hit chance in percent: flat 75 + 2 per level of attacker advantage,
/// clamped to 5–95.
pub fn hit_chance(attacker_level: u32, defender_level: u32) -> u32 {
    let diff = attacker_level as i32 - defender_level as i32;
    (75 + 2 * diff).clamp(5, 95) as u32
}

/// One auto-attack's damage: a level-based die plus small variance.
/// Level 1 averages ~5 (3 + d4); each level adds ~2. Never zero on a hit.
pub fn strike_damage(attacker_level: u32, rng: &mut Xorshift) -> u32 {
    let base = 2 + u64::from(attacker_level) + rng.below(u64::from(2 + attacker_level));
    base.max(1) as u32
}

/// Resolve one blow: roll hit chance, then damage on a hit.
pub fn resolve_strike(attacker_level: u32, defender_level: u32, rng: &mut Xorshift) -> (bool, u32) {
    if !rng.chance(hit_chance(attacker_level, defender_level)) {
        return (false, 0);
    }
    (true, strike_damage(attacker_level, rng))
}

/// XP awarded for killing a mob of `victim_level`.
pub fn xp_for_level(victim_level: u32) -> u32 {
    25 * victim_level.max(1)
}

/// Coin awarded for killing a mob of `victim_level`.
pub fn coin_for_level(victim_level: u32) -> u32 {
    5 * victim_level.max(1)
}

/// XP debt a PC takes on death: 10 × level, applied saturating.
pub fn death_debt(victim_level: u32) -> u32 {
    10 * victim_level.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_chance_baseline_and_edges() {
        assert_eq!(hit_chance(1, 1), 75);
        assert_eq!(hit_chance(5, 1), 83);
        assert_eq!(hit_chance(1, 5), 67);
        assert_eq!(hit_chance(50, 1), 95);
        assert_eq!(hit_chance(1, 50), 5);
    }

    #[test]
    fn strike_damage_positive_and_scales() {
        let mut rng = Xorshift::seed(3);
        for _ in 0..100 {
            assert!((1..=8).contains(&strike_damage(1, &mut rng)));
        }
        let mut rng = Xorshift::seed(3);
        let mut high = 0;
        for _ in 0..50 {
            high = high.max(strike_damage(10, &mut rng));
        }
        assert!(high > 8);
    }

    #[test]
    fn awards_scale_with_level() {
        assert_eq!(xp_for_level(1), 25);
        assert_eq!(coin_for_level(1), 5);
        assert_eq!(xp_for_level(5), 125);
        assert_eq!(death_debt(1), 10);
    }

    #[test]
    fn miss_deals_zero() {
        // Seed 1: first chance(5) roll — just assert the shape, not the value.
        let mut rng = Xorshift::seed(1);
        let (hit, amount) = resolve_strike(1, 50, &mut rng);
        if !hit {
            assert_eq!(amount, 0);
        } else {
            assert!(amount >= 1);
        }
    }
}
