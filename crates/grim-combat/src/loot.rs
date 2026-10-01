//! Global loot table: shared drops every mob corpse rolls against.
//!
//! The only tier that ships (area / mob-type / instance tiers are deferred).
//! Entries are rolled independently per kill: each entry with
//! `chance_pct >= roll(100)` spawns one object into the corpse.

use bevy::prelude::*;

/// One shared drop.
#[derive(Debug, Clone)]
pub struct LootEntry {
    /// Short name (`get` matching, inventory rows).
    pub name: String,
    /// Look paragraphs.
    pub description: Vec<String>,
    /// Extra `get` keywords.
    pub keywords: Vec<String>,
    /// Room-listing line (shown when the corpse is looked inside? no — the
    /// corpse's own line; items list under `look in`).
    pub room_description: String,
    /// Independent roll: spawn on `rng.below(100) < chance_pct`.
    pub chance_pct: u32,
}

/// The shared drop table, in seed order.
#[derive(Resource, Default)]
pub struct GlobalLoot(pub Vec<LootEntry>);

/// Seed the table: a common fang, an uncommon pelt, a rare pebble.
pub fn seed_loot(table: &mut GlobalLoot) {
    table.0.push(LootEntry {
        name: "wolf fang".into(),
        description: vec!["A sharp fang, still warm.".into()],
        keywords: vec!["fang".into(), "wolf".into()],
        room_description: "a sharp wolf fang".into(),
        chance_pct: 40,
    });
    table.0.push(LootEntry {
        name: "torn pelt".into(),
        description: vec!["A pelt torn in the fight.".into()],
        keywords: vec!["pelt".into(), "torn".into()],
        room_description: "a torn pelt".into(),
        chance_pct: 25,
    });
    table.0.push(LootEntry {
        name: "shiny pebble".into(),
        description: vec!["A pebble that catches the light.".into()],
        keywords: vec!["pebble".into(), "shiny".into()],
        room_description: "a shiny pebble".into(),
        chance_pct: 5,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_seeds_three_entries() {
        let mut table = GlobalLoot::default();
        seed_loot(&mut table);
        assert_eq!(table.0.len(), 3);
    }
}
