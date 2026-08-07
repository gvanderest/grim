//! Character build content: the registered race/class catalogues a MUD author
//! draws from at creation.
//!
//! Race and class are *data*, not code: a [`RaceDef`] / [`ClassDef`] is a plain
//! record addressed by a `slug`, and the [`RaceRegistry`] / [`ClassRegistry`]
//! resources hold the set in author-defined order. Each ships a seeded
//! [`Default`] so the engine works out of the box; an author overrides the set
//! by inserting the resource before `ScenePlugin`/`AuthPlugin` — both
//! `init_resource` them (scene for the WHO abbreviations, auth for the creation
//! menus), so insert ahead of the plugin group (mirrors
//! `grim_auth::ReservedNamePrefixes`, which `AuthPlugin` init_resources).
//!
//! Classes carry a **tier ladder**: only tier-1 classes are creatable, and each
//! names its tier-2 evolution via `evolves_to`. Reroll/evolution is not built
//! yet — see `docs/adr/0002-character-class-tiers.md`.

use bevy::prelude::Resource;

/// A playable race — a registered record, addressed by `slug`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaceDef {
    /// Stable identity, stored on the character (e.g. `"half-elf"`).
    pub slug: String,
    /// Display name (`"Half-Elf"`).
    pub name: String,
    /// Short form for score sheets / prompts (`"H.Elf"`).
    pub abbrev: String,
    /// One-line flavour shown on the creation menu.
    pub description: String,
}

/// A character class — a registered record on a **tier ladder**. Only tier-1
/// classes are creatable; `evolves_to` names the tier-2 slug a future reroll
/// would promote into (`None` for a tier-2 class).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDef {
    /// Stable identity, stored on the character (e.g. `"warrior"`).
    pub slug: String,
    /// Display name (`"Warrior"`).
    pub name: String,
    /// Short form for score sheets / prompts (`"War"`).
    pub abbrev: String,
    /// One-line flavour shown on the creation menu.
    pub description: String,
    /// Ladder rung: `1` = creatable base class, `2` = evolution target.
    pub tier: u8,
    /// The tier-2 slug this class rerolls into, or `None` for a tier-2 class.
    pub evolves_to: Option<String>,
}

/// The set of playable races, in author-defined display order. Insert a custom
/// one before `ScenePlugin` to override the seeded [`Default`].
#[derive(Resource, Clone, Debug)]
pub struct RaceRegistry(pub Vec<RaceDef>);

/// The set of classes (all tiers), in author-defined display order. Only tier-1
/// entries are offered at creation (see [`ClassRegistry::creatable`]). Insert a
/// custom one before `ScenePlugin` to override the seeded [`Default`].
#[derive(Resource, Clone, Debug)]
pub struct ClassRegistry(pub Vec<ClassDef>);

impl RaceRegistry {
    /// The race with this slug, if registered.
    pub fn get(&self, slug: &str) -> Option<&RaceDef> {
        self.0.iter().find(|r| r.slug == slug)
    }

    /// Every race, in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &RaceDef> {
        self.0.iter()
    }
}

impl ClassRegistry {
    /// The class with this slug, if registered (any tier).
    pub fn get(&self, slug: &str) -> Option<&ClassDef> {
        self.0.iter().find(|c| c.slug == slug)
    }

    /// Every class, in registration order (all tiers).
    pub fn iter(&self) -> impl Iterator<Item = &ClassDef> {
        self.0.iter()
    }

    /// Only the tier-1 classes — the ones a player may pick at creation.
    pub fn creatable(&self) -> impl Iterator<Item = &ClassDef> {
        self.0.iter().filter(|c| c.tier == 1)
    }
}

/// Terse constructor for a seeded [`RaceDef`].
fn race(slug: &str, name: &str, abbrev: &str, description: &str) -> RaceDef {
    RaceDef {
        slug: slug.into(),
        name: name.into(),
        abbrev: abbrev.into(),
        description: description.into(),
    }
}

/// Terse constructor for a seeded [`ClassDef`].
fn class(
    slug: &str,
    name: &str,
    abbrev: &str,
    tier: u8,
    evolves_to: Option<&str>,
    description: &str,
) -> ClassDef {
    ClassDef {
        slug: slug.into(),
        name: name.into(),
        abbrev: abbrev.into(),
        description: description.into(),
        tier,
        evolves_to: evolves_to.map(Into::into),
    }
}

impl Default for RaceRegistry {
    fn default() -> Self {
        Self(vec![
            race(
                "human",
                "Human",
                "Human",
                "Versatile and ambitious, humans adapt to any calling.",
            ),
            race(
                "elf",
                "Elf",
                "Elf",
                "Graceful and long-lived, attuned to magic and the wild.",
            ),
            race(
                "dwarf",
                "Dwarf",
                "Dwarf",
                "Stout and steadfast, masters of stone and forge.",
            ),
            race(
                "halfling",
                "Halfling",
                "Hling",
                "Small, nimble, and improbably lucky.",
            ),
            race(
                "half-elf",
                "Half-Elf",
                "H.Elf",
                "Caught between two worlds, charming and adaptable.",
            ),
            race(
                "half-orc",
                "Half-Orc",
                "H.Orc",
                "Powerful and fierce, forged by a hard heritage.",
            ),
            race(
                "gnome",
                "Gnome",
                "Gnome",
                "Curious tinkerers brimming with arcane invention.",
            ),
        ])
    }
}

impl Default for ClassRegistry {
    fn default() -> Self {
        Self(vec![
            // ── Tier 1 — creatable base classes ──────────────────────
            class(
                "warrior",
                "Warrior",
                "War",
                1,
                Some("champion"),
                "A master of weapons and armor, versatile in any battle.",
            ),
            class(
                "mage",
                "Mage",
                "Mag",
                1,
                Some("archmage"),
                "A scholar of arcane magic wielding spells from a spellbook.",
            ),
            class(
                "cleric",
                "Cleric",
                "Cle",
                1,
                Some("templar"),
                "A divine caster who heals allies and smites foes.",
            ),
            class(
                "thief",
                "Thief",
                "Thi",
                1,
                Some("assassin"),
                "A stealthy skirmisher who strikes from the shadows.",
            ),
            class(
                "ranger",
                "Ranger",
                "Rng",
                1,
                Some("warden"),
                "A hunter and tracker at home in the wilds, deadly at range.",
            ),
            // ── Tier 2 — reroll targets, NOT creatable ───────────────
            class(
                "champion",
                "Champion",
                "Cha",
                2,
                None,
                "A peerless warrior whose presence turns the tide of battle.",
            ),
            class(
                "archmage",
                "Archmage",
                "Arc",
                2,
                None,
                "A master of the arcane who bends raw magic to their will.",
            ),
            class(
                "templar",
                "Templar",
                "Tmp",
                2,
                None,
                "A holy warrior channeling divine power into blade and blessing.",
            ),
            class(
                "assassin",
                "Assassin",
                "Asn",
                2,
                None,
                "A silent killer who ends fights before they begin.",
            ),
            class(
                "warden",
                "Warden",
                "Wrd",
                2,
                None,
                "A guardian of the wilds, one with beast and terrain.",
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RaceRegistry ─────────────────────────────────────────────

    #[test]
    fn race_registry_seeds_all_seven() {
        let reg = RaceRegistry::default();
        assert_eq!(reg.0.len(), 7);
        let slugs: Vec<&str> = reg.iter().map(|r| r.slug.as_str()).collect();
        assert_eq!(
            slugs,
            vec!["human", "elf", "dwarf", "halfling", "half-elf", "half-orc", "gnome"]
        );
    }

    #[test]
    fn race_registry_get_by_slug() {
        let reg = RaceRegistry::default();
        assert_eq!(reg.get("dwarf").unwrap().name, "Dwarf");
        assert_eq!(reg.get("half-elf").unwrap().abbrev, "H.Elf");
        assert!(reg.get("orc").is_none());
    }

    // ── ClassRegistry ────────────────────────────────────────────

    #[test]
    fn class_registry_seeds_all_ten() {
        let reg = ClassRegistry::default();
        assert_eq!(reg.0.len(), 10);
    }

    #[test]
    fn class_registry_get_by_slug() {
        let reg = ClassRegistry::default();
        let warrior = reg.get("warrior").unwrap();
        assert_eq!(warrior.name, "Warrior");
        assert_eq!(warrior.tier, 1);
        assert_eq!(warrior.evolves_to.as_deref(), Some("champion"));
        let champion = reg.get("champion").unwrap();
        assert_eq!(champion.tier, 2);
        assert_eq!(champion.evolves_to, None);
        assert!(reg.get("bard").is_none());
    }

    #[test]
    fn class_registry_creatable_is_the_five_tier_one() {
        let reg = ClassRegistry::default();
        let creatable: Vec<&str> = reg.creatable().map(|c| c.slug.as_str()).collect();
        assert_eq!(
            creatable,
            vec!["warrior", "mage", "cleric", "thief", "ranger"]
        );
        assert!(reg.creatable().all(|c| c.tier == 1));
    }

    #[test]
    fn every_tier_one_class_evolves_to_a_registered_tier_two() {
        let reg = ClassRegistry::default();
        for c in reg.creatable() {
            let target = c.evolves_to.as_deref().expect("tier-1 must evolve");
            let evolved = reg.get(target).expect("evolution target registered");
            assert_eq!(evolved.tier, 2);
        }
    }
}
