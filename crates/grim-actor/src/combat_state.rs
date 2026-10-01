//! Being-side combat state: health, posture, engagement, slow.
//!
//! Owned by `grim-actor` (state lives with the being); the combat *rules* that
//! read it live in `grim-combat`. `Health`/`Posture` are ensured on every live
//! being at spawn/entry so combat systems can assume presence; `Engaged` is
//! present only while fighting.

use bevy::prelude::*;

/// Consumable hit points. PCs run `100/100`; mobs carry blueprint-tuned
/// values. Clamped to `max` by regen; death fires at zero (see `grim-combat`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    pub current: u32,
    pub max: u32,
}

impl Health {
    /// Full pool of `max`.
    pub fn full(max: u32) -> Self {
        Self { current: max, max }
    }

    /// The PC pool (100/100).
    pub fn pc() -> Self {
        Self::full(100)
    }

    /// Subtract, saturating at zero.
    pub fn damage(&mut self, amount: u32) {
        self.current = self.current.saturating_sub(amount);
    }

    /// Add, clamping at `max`.
    pub fn heal(&mut self, amount: u32) {
        self.current = (self.current + amount).min(self.max);
    }

    /// True at zero.
    pub fn is_dead(&self) -> bool {
        self.current == 0
    }
}

/// Body position. Only scales HP regen today (sitting ×2, sleeping ×4);
/// no defense change, no auto-attack change.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Posture {
    #[default]
    Standing,
    Sitting,
    Sleeping,
}

/// Live fight membership. Ordered target list: index 0 is the primary (the
/// being this round's auto-attack hits); `switch` reorders.
#[derive(Component, Clone, Debug, Default)]
pub struct Engaged {
    pub targets: Vec<Entity>,
}

impl Engaged {
    /// The primary target, if any.
    pub fn primary(&self) -> Option<Entity> {
        self.targets.first().copied()
    }

    /// Append `target` unless already listed. Index 0 is never disturbed.
    pub fn add(&mut self, target: Entity) {
        if !self.targets.contains(&target) {
            self.targets.push(target);
        }
    }

    /// Move `target` to index 0. True when it was listed.
    pub fn prioritize(&mut self, target: Entity) -> bool {
        if let Some(pos) = self.targets.iter().position(|&e| e == target) {
            self.targets.remove(pos);
            self.targets.insert(0, target);
            true
        } else {
            false
        }
    }
}

/// Flee/move reuse delay, in seconds. Set to 3.0 on every flee attempt and
/// engaged move; ticks down in `grim-actor` movement. While positive, flee
/// and engaged moves are refused with `combat.delay`.
#[derive(Component, Clone, Copy, Debug)]
pub struct CombatSlow {
    pub remaining: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_damage_heal_clamp() {
        let mut h = Health::pc();
        h.damage(30);
        assert_eq!(h.current, 70);
        h.damage(999);
        assert!(h.is_dead());
        h.heal(50);
        assert_eq!(h.current, 50);
        h.heal(999);
        assert_eq!(h.current, 100);
    }

    #[test]
    fn engaged_primary_and_prioritize() {
        let mut e = Engaged::default();
        assert_eq!(e.primary(), None);
        let (a, b) = (
            Entity::from_raw_u32(1).unwrap(),
            Entity::from_raw_u32(2).unwrap(),
        );
        e.add(a);
        e.add(b);
        e.add(a);
        assert_eq!(e.targets, vec![a, b]);
        assert!(e.prioritize(b));
        assert_eq!(e.targets, vec![b, a]);
        assert!(!e.prioritize(Entity::from_raw_u32(9).unwrap()));
    }
}
