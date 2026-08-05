//! World seeding from **area blueprints**.
//!
//! Area/room definitions live in `data/areas/*.json` (committed world content),
//! not in code. Each file is one area blueprint plus a `rooms` array of room
//! blueprints. They are read **from the filesystem at startup**, so a MUD author
//! can edit an area's JSON and restart without recompiling. The directory is
//! [`AreaBlueprintDir`] (default `data/areas`, resolved against the process's
//! working directory — `/opt/grim` in production, where the deploy ships the
//! `data/areas` folder alongside the binary).
//!
//! On startup every blueprint with `"canonical": true` is loaded — `canonical`
//! is, for now, simply the flag that gates startup loading (see
//! `docs/adr/0001-area-identity-and-instancing.md`).
//!
//! Every area and room carries a stable [`GrimId`] (`id`) in the file, and all
//! references — a room's `exits` and the area's `starting_room` — point at those
//! **ids**, never slugs. Slugs are only a human alias, so renaming a room's slug
//! never breaks a link.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bevy::log::{error, warn};
use bevy::prelude::*;
use grim::prelude::{
    Area, Cardinal, Description, Exits, GrimId, InRoom, Name as GrimName, Npc, Room, StartingRoom,
};
use serde::Deserialize;

/// Directory the area blueprints (`*.json`) are read from at startup. Resolved
/// against the process working directory when relative. Insert this resource
/// before [`seed_world`] runs to override the default (`data/areas`).
#[derive(Resource, Clone, Debug)]
pub struct AreaBlueprintDir(pub PathBuf);

impl Default for AreaBlueprintDir {
    fn default() -> Self {
        Self(PathBuf::from("data/areas"))
    }
}

/// An area definition on disk: the area itself plus its rooms.
#[derive(Deserialize)]
struct AreaBlueprint {
    /// Stable Grim ID — the identity that references point at.
    id: GrimId,
    slug: String,
    name: String,
    /// Only `canonical` areas are stamped into the world at startup.
    #[serde(default)]
    canonical: bool,
    /// Grim ID of the room new characters (and the ultimate fallback) start in.
    #[serde(default)]
    starting_room: Option<GrimId>,
    #[serde(default)]
    rooms: Vec<RoomBlueprint>,
}

/// A room definition within an [`AreaBlueprint`].
#[derive(Deserialize)]
struct RoomBlueprint {
    /// Stable Grim ID — what `exits`/`starting_room` reference.
    id: GrimId,
    slug: String,
    name: String,
    description: String,
    /// direction name (`"north"`) -> destination room **Grim ID** (not slug),
    /// so renaming a room's slug never breaks the link.
    #[serde(default)]
    exits: HashMap<String, GrimId>,
    #[serde(default)]
    npcs: Vec<NpcBlueprint>,
}

/// A non-player character placed in a room.
#[derive(Deserialize)]
struct NpcBlueprint {
    name: String,
    description: String,
}

/// Seed the initial world by reading area blueprints from [`AreaBlueprintDir`]
/// at startup. Loads every `canonical` area (in sorted filename order for
/// determinism), then sets [`StartingRoom`] from the first area that names a
/// resolvable one. Called once at startup.
pub fn seed_world(mut commands: Commands, dir: Option<Res<AreaBlueprintDir>>) {
    let dir = dir
        .map(|d| d.0.clone())
        .unwrap_or_else(|| AreaBlueprintDir::default().0);

    let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|entry| match entry {
                Ok(e) => Some(e.path()),
                Err(e) => {
                    error!("skipping unreadable entry in area dir {dir:?}: {e}");
                    None
                }
            })
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect(),
        Err(e) => {
            error!("cannot read area blueprint dir {dir:?}: {e} — no areas loaded");
            Vec::new()
        }
    };
    files.sort(); // deterministic load order

    let mut starting: Option<Entity> = None;
    for path in &files {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                error!("cannot read area blueprint {path:?}: {e} — skipping");
                continue;
            }
        };
        let blueprint: AreaBlueprint = match serde_json::from_str(&raw) {
            Ok(bp) => bp,
            Err(e) => {
                error!("skipping unparseable area blueprint {path:?}: {e}");
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
        // Fail fast, and loudly. The scene systems take `Res<StartingRoom>`, so a
        // missing resource would otherwise panic cryptically on the first tick.
        // A MUD with no reachable world cannot serve logins — surface the real
        // cause at startup instead.
        None => panic!(
            "no starting room resolved from area blueprints in {dir:?}: need at least one \
             `canonical` area declaring a resolvable `starting_room`"
        ),
    }
}

/// Stamp one area blueprint into the world: spawn the area, its rooms, wire
/// exits, place NPCs. Returns this area's starting-room entity, if it declared a
/// resolvable one.
fn spawn_area(commands: &mut Commands, bp: &AreaBlueprint) -> Option<Entity> {
    // Validate room ids BEFORE spawning anything: a duplicate id would otherwise
    // leave the first room spawned but unreferenced (exits wire only the last
    // entity for that id). Reject the whole area instead of a half-built one.
    let mut seen = HashSet::new();
    for r in &bp.rooms {
        if !seen.insert(r.id) {
            error!(
                "area '{}': duplicate room id {} — skipping area",
                bp.slug, r.id
            );
            return None;
        }
    }

    let area = commands
        .spawn(Area {
            id: bp.id,
            friendly_id: bp.slug.clone(),
            name: bp.name.clone(),
        })
        .id();

    // Pass 1: spawn rooms and record Grim ID -> entity so exits can resolve.
    let mut room_ents: HashMap<GrimId, Entity> = HashMap::new();
    for r in &bp.rooms {
        let entity = commands
            .spawn((
                Room {
                    id: r.id,
                    friendly_id: r.slug.clone(),
                    name: r.name.clone(),
                    description: r.description.clone(),
                    area,
                },
                GrimName(r.name.clone()),
                Exits::default(),
            ))
            .id();
        room_ents.insert(r.id, entity);
    }

    // Pass 2: wire exits (by Grim ID, within this area) and place NPCs.
    for r in &bp.rooms {
        let Some(&from) = room_ents.get(&r.id) else {
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
            match room_ents.get(target) {
                Some(&to) => {
                    exits.insert(cardinal, to);
                }
                // Cross-area / unknown targets are not wired yet — log and skip
                // rather than fail (see ADR-0001 dangling-exit handling).
                None => warn!(
                    "area '{}' room '{}': exit '{dir}' -> unknown room id {target}, skipped",
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
        .and_then(|gid| room_ents.get(&gid).copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo's committed area blueprints, resolved from this crate's manifest
    /// dir so the test works regardless of the process working directory.
    fn areas_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/areas")
    }

    #[test]
    fn haven_blueprint_parses_and_references_by_grim_id() {
        let raw = std::fs::read_to_string(areas_dir().join("haven.json")).unwrap();
        let bp: AreaBlueprint = serde_json::from_str(&raw).unwrap();
        assert_eq!(bp.slug, "haven");
        assert!(bp.canonical);
        assert_eq!(bp.rooms.len(), 3);

        let tavern = &bp.rooms[0];
        let square = &bp.rooms[1];
        assert_eq!(tavern.slug, "tavern");
        // starting_room references the tavern by Grim ID, not slug.
        assert_eq!(bp.starting_room, Some(tavern.id));
        // The tavern's north exit references the square by Grim ID.
        assert_eq!(tavern.exits.get("north"), Some(&square.id));
    }

    #[test]
    fn seed_spawns_areas_rooms_exits_and_starting_room() {
        let mut app = App::new();
        app.insert_resource(AreaBlueprintDir(areas_dir()));
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

    #[test]
    #[should_panic(expected = "no starting room resolved")]
    fn seed_panics_when_no_starting_room() {
        // An empty area dir yields no canonical area, so no StartingRoom can be
        // resolved. Seeding must fail fast rather than leave the resource unset
        // (which would panic cryptically in the scene systems).
        let empty = std::env::temp_dir().join(format!("grim_seed_empty_{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        let mut app = App::new();
        app.insert_resource(AreaBlueprintDir(empty));
        app.add_systems(Startup, seed_world);
        app.update();
    }
}
