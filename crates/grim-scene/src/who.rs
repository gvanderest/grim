//! Session-local list renderers: the MUD-style `who` list plus the `where`
//! and `areas` lists. Factored out of [`crate::command::handle_ingame`] to
//! hold that dispatcher under the line budget — the same one-file-per-list
//! shape as `sockets.rs` and `finger.rs`.

use std::cmp::Ordering;

use bevy::prelude::*;
use chrono::{DateTime, Utc};
use grim_actor::Linkdead;
use grim_core::components::Gender;

use crate::formatter::{self, WhoRow};
use crate::params::{PlayerChars, RoomResolver, SessionRes};

/// The WHO ordering keys for one online character.
struct WhoKey {
    is_admin: bool,
    level: u32,
    connected_at: DateTime<Utc>,
    /// Lower-cased name for a case-insensitive tiebreak.
    sort_name: String,
}

/// One online character's WHO data: the ordering [`WhoKey`] plus the
/// fully-computed [`WhoRow`] to render.
struct WhoData<'a> {
    key: WhoKey,
    row: WhoRow<'a>,
}

/// WHO ordering: admins first, alphabetical by name; then everyone else by
/// level DESC, connect-time ASC (oldest connection first), name ASC.
fn who_order(a: &WhoKey, b: &WhoKey) -> Ordering {
    match (a.is_admin, b.is_admin) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.sort_name.cmp(&b.sort_name),
        (false, false) => b
            .level
            .cmp(&a.level)
            .then_with(|| a.connected_at.cmp(&b.connected_at))
            .then_with(|| a.sort_name.cmp(&b.sort_name)),
    }
}

/// Map a [`Gender`] to its single-character WHO code.
fn gender_char(gender: Gender) -> &'static str {
    match gender {
        Gender::Male => "M",
        Gender::Female => "F",
        Gender::Neutral => "N",
    }
}

/// The MUD-style `who` list. Each online character renders as
/// `LLL G RRRRR CCC GGGGG Name Title` (admins show `IMM` for level; restrings
/// override columns — see [`WhoRow`]). Sort: admins first, alphabetical; then
/// everyone else by level DESC, connect-time ASC, name ASC. Linkdead characters
/// still appear, marked.
pub(crate) fn format_who(
    player_chars: &PlayerChars,
    linkdead: &Query<&Linkdead>,
    res: &SessionRes,
) -> String {
    let mut data: Vec<WhoData> = player_chars
        .iter()
        .filter_map(|(e, n, _, actor, character, connected)| {
            let ch = character?;
            // Race/level/gender live on the shared `Actor` base now.
            let actor = actor?;
            let is_admin = ch.is_admin();
            let race_abbrev = res
                .races
                .get(&actor.race)
                .map(|r| r.abbrev.clone())
                .unwrap_or_default();
            let class_abbrev = res
                .classes
                .get(&ch.class)
                .map(|c| c.abbrev.clone())
                .unwrap_or_default();
            let level_text = if is_admin {
                "IMM".to_string()
            } else {
                actor.level.to_string()
            };
            Some(WhoData {
                key: WhoKey {
                    is_admin,
                    level: actor.level,
                    // Fall back to creation time if (impossibly) unstamped, so
                    // the tiebreak stays deterministic rather than panicking.
                    connected_at: connected.map_or(ch.created_at, |c| c.0),
                    sort_name: n.0.to_lowercase(),
                },
                row: WhoRow {
                    level: level_text,
                    gender: gender_char(actor.gender).to_string(),
                    race: race_abbrev,
                    class: class_abbrev,
                    guild: String::new(),
                    name: n.0.clone(),
                    title: ch.title.clone(),
                    restrings: &ch.restrings,
                    linkdead: linkdead.get(e).is_ok(),
                },
            })
        })
        .collect();

    data.sort_by(|a, b| who_order(&a.key, &b.key));

    let rows: Vec<WhoRow> = data.into_iter().map(|d| d.row).collect();
    formatter::format_who_list(&rows)
}

/// The `where` list: other characters in the actor's current area, by room.
pub(crate) fn format_where(
    char_entity: Entity,
    player_chars: &PlayerChars,
    rooms: &RoomResolver,
) -> String {
    let actor_area = player_chars
        .get(char_entity)
        .ok()
        .and_then(|(_, _, ir, _, _, _)| rooms.rooms.get(ir.room).ok().map(|(_, r, _)| r.area));
    let mut entries: Vec<(String, String)> = Vec::new();
    if let Some(area) = actor_area {
        for (e, n, ir, _, _, _) in player_chars.iter() {
            if e == char_entity {
                continue;
            }
            if let Ok((_, r, rn)) = rooms.rooms.get(ir.room) {
                if r.area == area {
                    entries.push((n.0.clone(), rn.0.clone()));
                }
            }
        }
        entries.sort_by(|a, b| a.1.cmp(&b.1));
    }
    formatter::format_where_list(&entries)
}

/// The sorted, deduped `areas` list of `(friendly_id, name)`.
pub(crate) fn format_areas(rooms: &RoomResolver) -> String {
    let mut entries: Vec<(String, String)> = rooms
        .areas
        .iter()
        .map(|a| (a.friendly_id.clone(), a.name.clone()))
        .collect();
    entries.sort();
    entries.dedup();
    formatter::format_areas_list(&entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::cmp::Ordering;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn key(is_admin: bool, level: u32, connected: i64, name: &str) -> WhoKey {
        WhoKey {
            is_admin,
            level,
            connected_at: at(connected),
            sort_name: name.to_lowercase(),
        }
    }

    #[test]
    fn gender_char_maps_each_variant() {
        assert_eq!(gender_char(Gender::Male), "M");
        assert_eq!(gender_char(Gender::Female), "F");
        assert_eq!(gender_char(Gender::Neutral), "N");
    }

    #[test]
    fn admins_sort_before_non_admins() {
        // A level-1 admin outranks a level-99 player.
        assert_eq!(
            who_order(&key(true, 1, 100, "Zed"), &key(false, 99, 1, "Aaa")),
            Ordering::Less
        );
        assert_eq!(
            who_order(&key(false, 99, 1, "Aaa"), &key(true, 1, 100, "Zed")),
            Ordering::Greater
        );
    }

    #[test]
    fn admins_sort_alphabetically_case_insensitive() {
        assert_eq!(
            who_order(&key(true, 5, 1, "bob"), &key(true, 5, 1, "Alice")),
            Ordering::Greater
        );
    }

    #[test]
    fn non_admins_sort_by_level_desc_then_connect_then_name() {
        // Higher level first.
        assert_eq!(
            who_order(&key(false, 10, 5, "Bob"), &key(false, 5, 1, "Al")),
            Ordering::Less
        );
        // Same level → oldest connection (smaller timestamp) first.
        assert_eq!(
            who_order(&key(false, 10, 1, "Zed"), &key(false, 10, 9, "Al")),
            Ordering::Less
        );
        // Same level + same connect time → name ascending.
        assert_eq!(
            who_order(&key(false, 10, 5, "Al"), &key(false, 10, 5, "Bob")),
            Ordering::Less
        );
    }

    #[test]
    fn full_ordering_admins_then_level_then_connect() {
        let mut keys = [
            key(false, 10, 30, "Carol"),
            key(true, 1, 99, "Zara"),
            key(false, 10, 10, "Bob"),
            key(true, 1, 1, "Alice"),
            key(false, 5, 5, "Dave"),
        ];
        keys.sort_by(who_order);
        let order: Vec<&str> = keys.iter().map(|k| k.sort_name.as_str()).collect();
        // Admins alpha (alice, zara), then level-10 by connect (bob<carol), then
        // the level-5 player.
        assert_eq!(order, vec!["alice", "zara", "bob", "carol", "dave"]);
    }
}
