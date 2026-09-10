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

use bevy::log::{error, info, warn};
use bevy::prelude::*;
use grim::prelude::{
    compile, Actor, Area, Cardinal, CompiledTrigger, Creature, Description, Exits, Gender, GrimId,
    InRoom, Keywords, Name as GrimName, Object, Room, RoomDescription, ScriptTriggers,
    StartingRoom, TriggerDef,
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
    #[serde(default)]
    objects: Vec<ObjectBlueprint>,
}

/// A non-player character placed in a room.
#[derive(Deserialize)]
struct NpcBlueprint {
    name: String,
    /// Look paragraphs (`look <name>` shows them newline-joined).
    description: Vec<String>,
    /// Extra `look <keyword>` words (matched case-insensitively).
    #[serde(default)]
    keywords: Vec<String>,
    /// Room-listing line shown under the room description. Empty falls back
    /// to `"<name> is here."`.
    #[serde(default)]
    room_description: String,
    /// Scripted reactions (`{on, script}` with inline Lua). A script that
    /// fails to compile is logged and skipped — the mob still spawns.
    #[serde(default)]
    triggers: Vec<TriggerDef>,
}

/// A pickable object placed in a room.
#[derive(Deserialize)]
struct ObjectBlueprint {
    /// Short name: inventory rows, pickup lines, `get`/`drop` name matching.
    name: String,
    /// Look paragraphs (`look <name>` shows them newline-joined).
    description: Vec<String>,
    /// Extra `get <keyword>` words (matched case-insensitively).
    #[serde(default)]
    keywords: Vec<String>,
    /// Long room-listing line shown under the creatures. Empty falls back to
    /// the short name.
    #[serde(default)]
    room_description: String,
}

/// Seed the initial world by reading area blueprints from [`AreaBlueprintDir`]
/// at startup. Loads every `canonical` area (in sorted filename order for
/// determinism), then sets [`StartingRoom`] from the first area that names a
/// resolvable one. Called once at startup.
pub fn seed_world(mut commands: Commands, dir: Option<Res<AreaBlueprintDir>>) {
    let dir = dir
        .map(|d| d.0.clone())
        .unwrap_or_else(|| AreaBlueprintDir::default().0);

    // Load every canonical blueprint first so exits can resolve across areas
    // (ADR-0001: cross-area links bind Canonical → Canonical). Rooms are
    // stamped before any exit is wired, so an exit may target any loaded area.
    let blueprints = load_canonical_blueprints(&dir);

    // Pass 1: stamp every area and room, recording Grim ID -> entity globally.
    let mut room_ents: HashMap<GrimId, Entity> = HashMap::new();
    let mut pending: Vec<AreaBlueprint> = Vec::new();
    for bp in blueprints {
        if stamp_area_rooms(&mut commands, &bp, &mut room_ents).is_some() {
            pending.push(bp);
        }
    }

    // Pass 2: wire exits (by Grim ID, across all stamped areas) and place
    // NPCs/objects. A target that resolves to no stamped room is a dangling
    // exit: log and skip it, never fail startup over one bad link.
    for bp in &pending {
        wire_area_contents(&mut commands, bp, &room_ents);
    }
    let starting = pending
        .iter()
        .find_map(|bp| bp.starting_room.and_then(|id| room_ents.get(&id).copied()));

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

/// Read every `*.json` blueprint in `dir` (sorted filenames for determinism),
/// returning the `canonical` ones. Unreadable or unparseable files log and skip.
fn load_canonical_blueprints(dir: &PathBuf) -> Vec<AreaBlueprint> {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
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

    let mut blueprints = Vec::new();
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
        blueprints.push(blueprint);
    }
    blueprints
}

/// Stamp one area and its rooms, recording Grim ID -> entity in `room_ents`.
/// A duplicate room id — within the area or across already-stamped areas —
/// would leave one room spawned but unreferenced, so it rejects the whole
/// area instead of a half-built one. Returns the area entity.
fn stamp_area_rooms(
    commands: &mut Commands,
    bp: &AreaBlueprint,
    room_ents: &mut HashMap<GrimId, Entity>,
) -> Option<Entity> {
    let mut seen = HashSet::new();
    for r in &bp.rooms {
        if !seen.insert(r.id) {
            error!(
                "area '{}': duplicate room id {} — skipping area",
                bp.slug, r.id
            );
            return None;
        }
        if room_ents.contains_key(&r.id) {
            error!(
                "area '{}': room id {} already stamped by another area — skipping area",
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
    Some(area)
}

/// Wire one stamped area's exits against the global room map and place its
/// NPCs and objects.
fn wire_area_contents(
    commands: &mut Commands,
    bp: &AreaBlueprint,
    room_ents: &HashMap<GrimId, Entity>,
) {
    for r in &bp.rooms {
        let Some(&from) = room_ents.get(&r.id) else {
            continue;
        };
        commands.entity(from).insert(Exits {
            exits: resolve_exits(&bp.slug, &r.slug, &r.exits, room_ents),
        });

        for npc in &r.npcs {
            spawn_npc(commands, &bp.slug, npc, from);
        }
        for obj in &r.objects {
            commands.spawn((
                Object,
                GrimName(obj.name.clone()),
                Description(obj.description.clone()),
                Keywords(obj.keywords.clone()),
                RoomDescription(obj.room_description.clone()),
                InRoom { room: from },
            ));
        }
    }
}

/// Resolve one room's exits (direction name -> destination Grim ID) to room
/// entities. Bad directions and unknown targets log and skip — a dangling
/// exit never fails startup.
fn resolve_exits(
    area_slug: &str,
    room_slug: &str,
    exits: &HashMap<String, GrimId>,
    room_ents: &HashMap<GrimId, Entity>,
) -> HashMap<Cardinal, Entity> {
    let mut resolved = HashMap::new();
    for (dir, target) in exits {
        let Some(cardinal) = Cardinal::parse(dir) else {
            warn!("area '{area_slug}' room '{room_slug}': bad exit direction '{dir}'");
            continue;
        };
        match room_ents.get(target) {
            Some(&to) => {
                resolved.insert(cardinal, to);
            }
            None => warn!(
                "area '{area_slug}' room '{room_slug}': exit '{dir}' -> unknown room id {target}, skipped"
            ),
        }
    }
    resolved
}

/// Stamp one NPC blueprint into `room`: the being bundle plus its compiled
/// script triggers. A trigger that fails to compile is logged and skipped —
/// the mob still spawns, triggerless for that moment.
fn spawn_npc(commands: &mut Commands, area_slug: &str, npc: &NpcBlueprint, room: Entity) {
    // Compile each trigger once now: a typo fails loudly at startup (logged,
    // trigger skipped) instead of on a player's move.
    let mut compiled = Vec::with_capacity(npc.triggers.len());
    for def in &npc.triggers {
        match compile(&def.script) {
            Ok(bytecode) => compiled.push(CompiledTrigger {
                on: def.on,
                bytecode,
            }),
            Err(error) => error!(
                "area '{area_slug}' npc '{}': skipping trigger that does not compile: {error}",
                npc.name
            ),
        }
    }
    let mut mob = commands.spawn((
        Creature,
        // Seeded mobs carry the shared `Actor` base with sensible
        // defaults (no race/build data in blueprints yet): empty race,
        // level 1, neutral gender.
        Actor {
            race: String::new(),
            level: 1,
            gender: Gender::Neutral,
        },
        GrimName(npc.name.clone()),
        Description(npc.description.clone()),
        Keywords(npc.keywords.clone()),
        RoomDescription(npc.room_description.clone()),
        InRoom { room },
    ));
    if !compiled.is_empty() {
        info!(
            "area '{area_slug}' npc '{}': {} script trigger(s) loaded",
            npc.name,
            compiled.len()
        );
        mob.insert(ScriptTriggers(compiled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim::prelude::{CarriedBy, TriggerKind};

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
        assert_eq!(bp.rooms.len(), 11);

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

        // Two areas: Haven (11 rooms) plus Whisperwood (4 rooms).
        let areas = app.world_mut().query::<&Area>().iter(app.world()).count();
        assert_eq!(areas, 2);
        let rooms: Vec<String> = app
            .world_mut()
            .query::<&Room>()
            .iter(app.world())
            .map(|r| r.friendly_id.clone())
            .collect();
        assert_eq!(rooms.len(), 15);
        assert!(rooms.contains(&"tavern".to_string()));
        assert!(rooms.contains(&"bear-cavern".to_string()));

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
        // Cross-area exits are covered in `seed_wires_exits_across_areas`.

        // Both mobs are present: Grimmok in Haven, the bear in Whisperwood.
        let creatures = app
            .world_mut()
            .query::<&Creature>()
            .iter(app.world())
            .count();
        assert_eq!(creatures, 2);

        // Grimmok spawned with both greeting triggers compiled.
        let mut scripted = app.world_mut().query::<&ScriptTriggers>();
        let triggers = scripted.iter(app.world()).next().expect("Grimmok scripted");
        assert_eq!(triggers.0.len(), 2);
        assert!(triggers.0.iter().any(|t| t.on == TriggerKind::Enter));
        assert!(triggers.0.iter().any(|t| t.on == TriggerKind::AttemptLeave));

        // Grimmok carries its look paragraphs, keywords, and room line.
        let mut descs = app.world_mut().query::<(&GrimName, &Description)>();
        let (_, desc) = descs
            .iter(app.world())
            .find(|(name, _)| name.0 == "Grimmok Ironhand")
            .unwrap();
        assert_eq!(desc.0.len(), 2);
        let mut bear_rooms = app.world_mut().query::<(&GrimName, &InRoom)>();
        let (_, bear_room) = bear_rooms
            .iter(app.world())
            .find(|(name, _)| name.0 == "Old Cave Bear")
            .expect("bear spawned");
        assert_eq!(
            app.world().get::<Room>(bear_room.room).unwrap().friendly_id,
            "bear-cavern"
        );
        let mut keys = app.world_mut().query::<(&GrimName, &Keywords)>();
        assert!(keys
            .iter(app.world())
            .find(|(name, _)| name.0 == "Grimmok Ironhand")
            .unwrap()
            .1
             .0
            .contains(&"smith".to_string()));
        let mut lines = app.world_mut().query::<(&GrimName, &RoomDescription)>();
        assert_eq!(
            lines
                .iter(app.world())
                .find(|(name, _)| name.0 == "Grimmok Ironhand")
                .unwrap()
                .1
                 .0,
            "Grimmok Ironhand stands here, hammering metal."
        );
    }

    #[test]
    fn seed_wires_exits_across_areas() {
        let mut app = App::new();
        app.insert_resource(AreaBlueprintDir(areas_dir()));
        app.add_systems(Startup, seed_world);
        app.update();

        // Haven's east road reaches Whisperwood's forest edge, and back.
        let (road_entity, _) = app
            .world_mut()
            .query::<(Entity, &Room)>()
            .iter(app.world())
            .find(|(_, r)| r.friendly_id == "east-road")
            .unwrap();
        let road_exits = app.world().get::<Exits>(road_entity).unwrap();
        let east = road_exits.exits.get(&Cardinal::East).copied().unwrap();
        assert_eq!(
            app.world().get::<Room>(east).unwrap().friendly_id,
            "forest-edge"
        );
        let edge_exits = app.world().get::<Exits>(east).unwrap();
        let west = edge_exits.exits.get(&Cardinal::West).copied().unwrap();
        assert_eq!(
            app.world().get::<Room>(west).unwrap().friendly_id,
            "east-road"
        );
    }

    #[test]
    fn seed_spawns_blueprint_objects_in_their_room() {
        let mut app = App::new();
        app.insert_resource(AreaBlueprintDir(areas_dir()));
        app.add_systems(Startup, seed_world);
        app.update();

        // The tavern's brass lantern: short name, keywords, long room line,
        // placed in the tavern with no carrier.
        let mut objects = app.world_mut().query::<(
            Entity,
            &Object,
            &GrimName,
            &Keywords,
            &RoomDescription,
            &InRoom,
        )>();
        let found: Vec<(String, Vec<String>, String, Entity)> = objects
            .iter(app.world())
            .map(|(_, _, nm, kw, line, ir)| (nm.0.clone(), kw.0.clone(), line.0.clone(), ir.room))
            .collect();
        assert_eq!(found.len(), 1);
        let (name, keywords, line, room) = &found[0];
        assert_eq!(name, "brass lantern");
        assert!(keywords.contains(&"lantern".to_string()));
        assert_eq!(
            line,
            "A brass lantern rests here, its glass dusty but intact."
        );
        assert_eq!(
            app.world().get::<Room>(*room).unwrap().friendly_id,
            "tavern"
        );
        let mut carried = app.world_mut().query::<&CarriedBy>();
        assert!(carried.iter(app.world()).next().is_none());
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
