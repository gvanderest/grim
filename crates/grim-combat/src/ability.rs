//! Ability definitions: skills and spells share one shape.
//!
//! The only dispatch difference is the verb (`kick goblin` vs
//! `cast fireball goblin`); everything else — combat-only gating, legitimate
//! targets, fight initiation, damage, cooldown — is registration config.
//! Defs live vertical-side in `grim-combat` until a second (non-combat)
//! ability consumer justifies carving out `grim-skills` (ADR-0007).

use std::collections::HashMap;

use bevy::prelude::*;

/// Skill (direct verb) vs spell (`cast <slug>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityKind {
    Skill,
    Spell,
}

/// Who an ability may target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityTarget {
    /// Self only (heals, buffs — future).
    #[allow(dead_code)]
    OwnSelf,
    /// An engaged foe.
    Foe,
    /// Any other being (future).
    #[allow(dead_code)]
    Ally,
    /// Anything, including corpses (future).
    #[allow(dead_code)]
    Any,
}

/// One registered ability.
#[derive(Debug, Clone)]
pub struct AbilityDef {
    /// Stable slug (`"kick"`), matched against the verb / `cast` argument.
    pub slug: String,
    /// Display name for output lines.
    pub name: String,
    pub kind: AbilityKind,
    /// True: only usable while engaged in combat.
    pub combat_only: bool,
    /// True: using it on an unengaged target starts a fight. `kick` is false.
    pub initiates: bool,
    pub target: AbilityTarget,
    /// Damage multiplier over a plain strike (1.5 = kick's edge).
    pub damage_mult: f32,
    /// Reuse delay in seconds.
    pub cooldown_secs: f32,
}

/// All registered abilities, in registration order.
#[derive(Resource, Default)]
pub struct AbilityRegistry(pub Vec<AbilityDef>);

impl AbilityRegistry {
    /// Look up by slug, case-insensitive.
    pub fn get(&self, slug: &str) -> Option<&AbilityDef> {
        self.0.iter().find(|a| a.slug.eq_ignore_ascii_case(slug))
    }
}

/// Per-(being, ability) reuse timestamps. Values are seconds on the combat
/// clock (accumulated `Time` elapsed); a use is allowed when
/// `now - last >= cooldown_secs`.
#[derive(Resource, Default)]
pub struct AbilityCooldowns(pub HashMap<(Entity, String), f32>);

/// Monotonic combat clock, advanced by the round system. Cooldowns read it
/// so tests control time by writing the resource.
#[derive(Resource, Default)]
pub struct CombatClock(pub f32);

/// Seed the registry with the universal `kick` skill. Every tier-1 class
/// grants it at level 1 (see `ClassRegistry`); the grant check itself lives
/// in the kick handler (class slug + actor level).
pub fn seed_registry(registry: &mut AbilityRegistry) {
    registry.0.push(AbilityDef {
        slug: "kick".into(),
        name: "kick".into(),
        kind: AbilityKind::Skill,
        combat_only: true,
        initiates: false,
        target: AbilityTarget::Foe,
        damage_mult: 1.5,
        cooldown_secs: 6.0,
    });
}

/// Whether `class_slug` grants `ability_slug` at `level`: the class def must
/// list the slug at a granted level `<= level`.
pub fn class_grants(
    classes: &grim_world::ClassRegistry,
    class_slug: &str,
    ability_slug: &str,
    level: u32,
) -> bool {
    classes.get(class_slug).is_some_and(|def| {
        def.skills
            .iter()
            .any(|(s, l)| s == ability_slug && *l <= level)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lookup_case_insensitive() {
        let mut reg = AbilityRegistry::default();
        seed_registry(&mut reg);
        assert!(reg.get("kick").is_some());
        assert!(reg.get("KICK").is_some());
        assert!(reg.get("punch").is_none());
    }

    #[test]
    fn kick_def_shape() {
        let mut reg = AbilityRegistry::default();
        seed_registry(&mut reg);
        let kick = reg.get("kick").unwrap();
        assert_eq!(kick.kind, AbilityKind::Skill);
        assert!(kick.combat_only);
        assert!(!kick.initiates);
    }

    #[test]
    fn grants_follow_class_data() {
        let classes = grim_world::ClassRegistry::default();
        assert!(class_grants(&classes, "warrior", "kick", 1));
        assert!(class_grants(&classes, "mage", "kick", 1));
        assert!(!class_grants(&classes, "warrior", "fireball", 1));
        assert!(!class_grants(&classes, "nope", "kick", 1));
    }
}
