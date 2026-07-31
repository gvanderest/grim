//! World seeding from **area blueprints**.
//!
//! Area/room definitions live in `data/areas/*.json` (committed world content),
//! not in code. Each file is one area blueprint plus a `rooms` array of room
//! blueprints. The files are baked into the binary with `include_str!` so the
//! static musl build and the test harness need no runtime files; editing an
//! area still means editing its JSON (and a rebuild).
//!
//! On startup every blueprint with `"canonical": true` is loaded — `canonical`
//! is, for now, simply the flag that gates startup loading (see
//! `docs/adr/0001-area-identity-and-instancing.md`). Each loaded area and room
//! is assigned a fresh [`GrimId`]; ids are not persisted for areas, so a restart
//! reseeds them.

use std::collections::HashMap;

use bevy::log::{error, warn};
use bevy::prelude::*;
use grim::prelude::{
    Area, Cardinal, Description, Exits, GrimId, InRoom, Name as GrimName, Npc, Room, StartingRoom,
};
use serde::Deserialize;

/// Every committed area blueprint, baked in at build time.
const AREA_BLUEPRINTS: &[&str] = &[include_str!("../../../data/areas/haven.json")];

/// An area definition on disk: the area itself plus its rooms.
#[derive(Deserialize)]
struct AreaBlueprint {
    slug: String,
    name: String,
    /// Only `canonical` areas are stamped into the world at startup.
    #[serde(default)]
    canonical: bool,
    /// Slug of the room new characters (and the ultimate fallback) start in.
    #[serde(default)]
    starting_room: Option<String>,
    #[serde(default)]
    rooms: Vec<RoomBlueprint>,
}

/// A room definition within an [`AreaBlueprint`].
#[derive(Deserialize)]
struct RoomBlueprint {
    slug: String,
    name: String,
    description: String,
    /// direction name (`"north"`) -> destination room slug, within this area.
    #[serde(default)]
    exits: HashMap<String, String>,
    #[serde(default)]
    npcs: Vec<NpcBlueprint>,
}

/// A non-player character placed in a room.
#[derive(Deserialize)]
struct NpcBlueprint {
    name: String,
    description: String,
}

/// Seed the initial world from the baked area blueprints. Loads every
/// `canonical` area, then sets [`StartingRoom`] from the first area that names a
/// resolvable one. Called once at startup.
pub fn seed_world(mut commands: Commands) {
    let mut starting: Option<Entity> = None;

    for raw in AREA_BLUEPRINTS {
        let blueprint: AreaBlueprint = match serde_json::from_str(raw) {
            Ok(bp) => bp,
            Err(e) => {
                error!("skipping unparseable area blueprint: {e}");
                continue;
            }
        };
        // `canonical` gates startup loading (instancing is not built yet).
        if !blueprint.canonical {
            continue;
        }
        if let Some(room) = spawn_area(&mut commands, &blueprint) {
            starting = starting.or(Some(room));
        }
    }

    match starting {
        Some(room) => commands.insert_resource(StartingRoom(room)),
        None => error!("no starting room resolved from area blueprints — logins will fail"),
    }
}

/// Stamp one area blueprint into the world: spawn the area, its rooms, wire
/// exits, place NPCs. Returns this area's starting-room entity, if it declared a
/// resolvable one.
fn spawn_area(commands: &mut Commands, bp: &AreaBlueprint) -> Option<Entity> {
    let area = commands
        .spawn(Area {
            id: GrimId::new(),
            friendly_id: bp.slug.clone(),
            name: bp.name.clone(),
        })
        .id();

    // Pass 1: spawn rooms and record slug -> entity so exits can resolve.
    let mut room_ents: HashMap<&str, Entity> = HashMap::new();
    for r in &bp.rooms {
        let entity = commands
            .spawn((
                Room {
                    id: GrimId::new(),
                    friendly_id: r.slug.clone(),
                    name: r.name.clone(),
                    description: r.description.clone(),
                    area,
                },
                GrimName(r.name.clone()),
                Exits::default(),
            ))
            .id();
        if room_ents.insert(&r.slug, entity).is_some() {
            warn!("area '{}': duplicate room slug '{}'", bp.slug, r.slug);
        }
    }

    // Pass 2: wire exits (within this area) and place NPCs.
    for r in &bp.rooms {
        let Some(&from) = room_ents.get(r.slug.as_str()) else {
            continue;
        };
        let mut exits = HashMap::new();
        for (dir, target) in &r.exits {
            let Some(cardinal) = Cardinal::parse(dir) else {
                warn!(
                    "area '{}' room '{}': bad exit direction '{dir}'",
                    bp.slug, r.slug
                );
                continue;
            };
            match room_ents.get(target.as_str()) {
                Some(&to) => {
                    exits.insert(cardinal, to);
                }
                // Cross-area / unknown targets are not wired yet — log and skip
                // rather than fail (see ADR-0001 dangling-exit handling).
                None => warn!(
                    "area '{}' room '{}': exit '{dir}' -> unknown room '{target}', skipped",
                    bp.slug, r.slug
                ),
            }
        }
        commands.entity(from).insert(Exits { exits });

        for npc in &r.npcs {
            commands.spawn((
                Npc,
                GrimName(npc.name.clone()),
                Description(npc.description.clone()),
                InRoom { room: from },
            ));
        }
    }

    bp.starting_room
        .as_deref()
        .and_then(|slug| room_ents.get(slug).copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haven_blueprint_parses() {
        let bp: AreaBlueprint = serde_json::from_str(AREA_BLUEPRINTS[0]).unwrap();
        assert_eq!(bp.slug, "haven");
        assert!(bp.canonical);
        assert_eq!(bp.starting_room.as_deref(), Some("tavern"));
        assert_eq!(bp.rooms.len(), 3);
    }

    #[test]
    fn seed_spawns_areas_rooms_exits_and_starting_room() {
        let mut app = App::new();
        app.add_systems(Startup, seed_world);
        app.update();

        // One area, three rooms.
        let areas = app.world_mut().query::<&Area>().iter(app.world()).count();
        assert_eq!(areas, 1);
        let rooms: Vec<String> = app
            .world_mut()
            .query::<&Room>()
            .iter(app.world())
            .map(|r| r.friendly_id.clone())
            .collect();
        assert_eq!(rooms.len(), 3);
        assert!(rooms.contains(&"tavern".to_string()));

        // Starting room resolved and points at the tavern.
        let starting = app.world().resource::<StartingRoom>().0;
        let name = app
            .world()
            .get::<Room>(starting)
            .unwrap()
            .friendly_id
            .clone();
        assert_eq!(name, "tavern");

        // Tavern's north exit is wired to the square.
        let (tavern_entity, _) = app
            .world_mut()
            .query::<(Entity, &Room)>()
            .iter(app.world())
            .find(|(_, r)| r.friendly_id == "tavern")
            .unwrap();
        let exits = app.world().get::<Exits>(tavern_entity).unwrap();
        let north = exits.exits.get(&Cardinal::North).copied().unwrap();
        assert_eq!(
            app.world().get::<Room>(north).unwrap().friendly_id,
            "square"
        );

        // The NPC is present.
        let npcs = app.world_mut().query::<&Npc>().iter(app.world()).count();
        assert_eq!(npcs, 1);
    }
}
