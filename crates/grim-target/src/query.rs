//! Selector application: rank candidates, sort best-first, take what the
//! [`TargetSpec`](crate::TargetSpec) asks for.
//!
//! Candidates are `(entity, name, keywords)` triples from whatever query the
//! caller already ran (room occupants, ground objects, a pack). Ties fall to
//! the lowest entity id, keeping resolution deterministic.

use bevy::prelude::Entity;

use crate::parse::{Selector, TargetSpec};
use crate::rank::{rank_match, Rank};

pub fn query<'a>(
    spec: &TargetSpec,
    candidates: impl Iterator<Item = (Entity, &'a str, &'a [String])>,
) -> Vec<Entity> {
    if spec.selector == Selector::All && spec.terms.is_empty() {
        // Bare `all`: no terms to rank by, so entity-id order is the order.
        let mut all: Vec<(u64, Entity)> = candidates
            .map(|(entity, _, _)| (entity.to_bits(), entity))
            .collect();
        all.sort();
        return all.into_iter().map(|(_, entity)| entity).collect();
    }
    let mut scored: Vec<(Rank, u64, Entity)> = candidates
        .filter_map(|(entity, name, keywords)| {
            rank_match(name, keywords, &spec.terms).map(|rank| (rank, entity.to_bits(), entity))
        })
        .collect();
    scored.sort();
    let ordered: Vec<Entity> = scored.into_iter().map(|(_, _, entity)| entity).collect();
    match spec.selector {
        Selector::One => ordered.into_iter().take(1).collect(),
        Selector::Nth(nth) => ordered.into_iter().nth(nth.get() - 1).into_iter().collect(),
        Selector::All => ordered,
        Selector::Many(count) => ordered.into_iter().take(count.get()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{parse_target, ParseOptions};

    fn world_with(count: usize) -> (bevy::prelude::App, Vec<Entity>) {
        let mut app = bevy::prelude::App::new();
        let entities = (0..count)
            .map(|_| app.world_mut().spawn_empty().id())
            .collect::<Vec<_>>();
        // Names live outside the world here; the caller pairs them back by
        // index when feeding `query`.
        (app, entities)
    }

    fn spec_of(raw: &str) -> TargetSpec {
        parse_target(raw, ParseOptions::ITEM).unwrap()
    }

    #[test]
    fn one_returns_best_match() {
        let (_app, entities) = world_with(2);
        let names = ["Wrackus".to_string(), "Wrack".to_string()];
        let keywords: Vec<Vec<String>> = vec![Vec::new(), Vec::new()];
        let found = query(
            &spec_of("wrack"),
            entities
                .iter()
                .enumerate()
                .map(|(i, e)| (*e, names[i].as_str(), keywords[i].as_slice())),
        );
        assert_eq!(found, vec![entities[1]]);
    }

    #[test]
    fn nth_returns_second_match() {
        let (_app, entities) = world_with(3);
        // Exact ("potion") outranks prefix ("potions") regardless of entity
        // ids, so the offset order is deterministic.
        let names = [
            "potions".to_string(),
            "potion".to_string(),
            "sword".to_string(),
        ];
        let keywords: Vec<Vec<String>> = vec![Vec::new(), Vec::new(), Vec::new()];
        let found = query(
            &spec_of("2.potion"),
            entities
                .iter()
                .enumerate()
                .map(|(i, e)| (*e, names[i].as_str(), keywords[i].as_slice())),
        );
        assert_eq!(found, vec![entities[0]]);
    }

    #[test]
    fn nth_past_the_end_is_empty() {
        let (_app, entities) = world_with(1);
        let names = ["potion".to_string()];
        let keywords: Vec<Vec<String>> = vec![Vec::new()];
        let found = query(
            &spec_of("2.potion"),
            entities
                .iter()
                .enumerate()
                .map(|(i, e)| (*e, names[i].as_str(), keywords[i].as_slice())),
        );
        assert!(found.is_empty());
    }

    #[test]
    fn many_caps_at_quantity() {
        let (_app, entities) = world_with(3);
        let names = ["coin".to_string(), "coin".to_string(), "coin".to_string()];
        let keywords: Vec<Vec<String>> = vec![Vec::new(), Vec::new(), Vec::new()];
        let found = query(
            &spec_of("2*coin"),
            entities
                .iter()
                .enumerate()
                .map(|(i, e)| (*e, names[i].as_str(), keywords[i].as_slice())),
        );
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn all_with_terms_ranks_best_first() {
        let (_app, entities) = world_with(2);
        let names = ["coin pouch".to_string(), "coin".to_string()];
        let keywords: Vec<Vec<String>> = vec![Vec::new(), Vec::new()];
        let found = query(
            &spec_of("all coin"),
            entities
                .iter()
                .enumerate()
                .map(|(i, e)| (*e, names[i].as_str(), keywords[i].as_slice())),
        );
        assert_eq!(found, vec![entities[1], entities[0]]);
    }

    #[test]
    fn bare_all_returns_everything_in_id_order() {
        let (_app, entities) = world_with(2);
        let names = ["zebra".to_string(), "apple".to_string()];
        let keywords: Vec<Vec<String>> = vec![Vec::new(), Vec::new()];
        let found = query(
            &spec_of("all"),
            entities
                .iter()
                .enumerate()
                .map(|(i, e)| (*e, names[i].as_str(), keywords[i].as_slice())),
        );
        let mut ordered = entities.clone();
        ordered.sort_by_key(|e| e.to_bits());
        assert_eq!(found, ordered);
    }

    #[test]
    fn no_match_is_empty() {
        let (_app, entities) = world_with(1);
        let names = ["sword".to_string()];
        let keywords: Vec<Vec<String>> = vec![Vec::new()];
        let found = query(
            &spec_of("potion"),
            entities
                .iter()
                .enumerate()
                .map(|(i, e)| (*e, names[i].as_str(), keywords[i].as_slice())),
        );
        assert!(found.is_empty());
    }
}
