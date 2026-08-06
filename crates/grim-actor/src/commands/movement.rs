//! Movement: walking an exit (`move`) and the admin `goto` teleport, plus the
//! shared [`place_actor`] seam every "put actor in room X" path routes through.
//! Both read the actor's [`Character`]/[`InRoom`] and resolve destinations
//! against `grim_world`'s room topology + address lookups.

use bevy::prelude::*;
use grim_engine_types::events::{Command, EngineCommand, InfoMessage, LookRoom, MoveEvent};
use grim_world::{
    resolve_room_address, room_location, Area, Exits, Room, RoomLocation, RoomLookup,
};

use crate::character::Character;
use crate::placement::InRoom;

/// Build a room's persisted [`RoomLocation`] from its `Room` + `Area` records.
/// `handle_move` reaches this shape via `grim_world::room_location` (which holds
/// `Query<&Room>`/`Query<&Area>`); `handle_goto` holds `Query<(Entity, &Room)>`
/// for address resolution, so it decomposes to the same records and shares this
/// one builder — keeping both paths in agreement if `RoomLocation` grows a field.
fn persisted_location(room: &Room, area: &Area) -> RoomLocation {
    RoomLocation {
        area: area.friendly_id.clone(),
        room: room.friendly_id.clone(),
    }
}

/// The single seam every "put actor in room X" path routes through: set the
/// actor's `InRoom` and refresh their persisted location. Per ADR-0001 the
/// location update is a property of the *destination*, not of how the actor
/// arrived, so walk and `goto` (and, later, summon/recall/login) share it. `loc`
/// is the destination's persisted [`RoomLocation`], precomputed by the caller.
///
/// Persists only `last_room` today. ADR-0001's `last_canonical_room` is not a
/// field yet; while every room is Canonical (no instancing) the two would be
/// equal, so it is deferred to the instancing work rather than added dead here.
fn place_actor(
    actor: Entity,
    to: Entity,
    loc: Option<RoomLocation>,
    inroom: &mut Query<&mut InRoom>,
    characters: &mut Query<&mut Character>,
) {
    if let Ok(mut ir) = inroom.get_mut(actor) {
        ir.room = to;
    }
    if let Some(loc) = loc {
        if let Ok(mut character) = characters.get_mut(actor) {
            character.last_room = Some(loc);
        }
    }
}

/// `move <direction>`: traverse an exit, emitting a movement event and an
/// automatic look at the destination. Also refreshes the character's persisted
/// `last_room` so a restart/copyover resumes them where they walked to.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_move(
    mut engine: MessageReader<EngineCommand>,
    mut inroom: Query<&mut InRoom>,
    exits: Query<&Exits>,
    rooms: Query<&Room>,
    areas: Query<&Area>,
    mut characters: Query<&mut Character>,
    mut move_ev: MessageWriter<MoveEvent>,
    mut look_room: MessageWriter<LookRoom>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Move { direction } = cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let from = match inroom.get(actor) {
            Ok(ir) => ir.room,
            Err(_) => continue,
        };
        match exits.get(from) {
            Ok(room_exits) => match room_exits.exits.get(&direction).copied() {
                Some(to) => {
                    // Keep the persisted location current on every step so an
                    // unexpected restart or copyover resumes the character in the
                    // room they actually walked to, not a stale one.
                    let loc = room_location(to, &rooms, &areas);
                    place_actor(actor, to, loc, &mut inroom, &mut characters);
                    move_ev.write(MoveEvent {
                        actor,
                        from,
                        to,
                        direction,
                    });
                    look_room.write(LookRoom {
                        target: actor,
                        room: to,
                    });
                }
                None => {
                    info.write(InfoMessage {
                        target: actor,
                        text: "You can't go that way.\n".into(),
                    });
                }
            },
            Err(_) => {
                info.write(InfoMessage {
                    target: actor,
                    text: "You can't go that way.\n".into(),
                });
            }
        }
    }
}

/// `goto <address>`: admin-only teleport. Resolves the address through
/// [`resolve_room_address`] and places the actor via the shared [`place_actor`]
/// seam, then shows the destination room.
///
/// Admin-gated here as defense in depth: the dispatcher already masks `goto` as
/// an unknown command for non-admins, so a well-behaved session never sends this
/// for one. A `goto` from a non-client source with no admin character is refused
/// silently (emitting anything would leak that the command exists).
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_goto(
    mut engine: MessageReader<EngineCommand>,
    mut inroom: Query<&mut InRoom>,
    rooms: Query<(Entity, &Room)>,
    areas: Query<(Entity, &Area)>,
    mut characters: Query<&mut Character>,
    mut look_room: MessageWriter<LookRoom>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Goto { target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let is_admin = characters.get(actor).map(|c| c.is_admin()).unwrap_or(false);
        if !is_admin {
            continue;
        }
        match resolve_room_address(target, &rooms, &areas) {
            RoomLookup::Found(to) => {
                let loc = rooms.get(to).ok().and_then(|(_, r)| {
                    areas
                        .get(r.area)
                        .ok()
                        .map(|(_, a)| persisted_location(r, a))
                });
                place_actor(actor, to, loc, &mut inroom, &mut characters);
                look_room.write(LookRoom {
                    target: actor,
                    room: to,
                });
            }
            RoomLookup::NotFound => {
                // `target` is raw admin input; escape it so it can't inject
                // colour markup into the reply (only the admin's own session,
                // but keep the invariant that interpolated input is escaped).
                info.write(InfoMessage {
                    target: actor,
                    text: format!("No room matches '{}'.\n", grim_color::escape_codes(target)),
                });
            }
            RoomLookup::Ambiguous(candidates) => {
                // List every candidate with its distinguishing ids so the admin
                // can re-issue `goto` against a unique one (an entity or grim id).
                let mut text = String::from("Select an option...\n");
                for e in candidates {
                    if let Ok((_, r)) = rooms.get(e) {
                        text.push_str(&room_ident_line(e, r));
                        text.push('\n');
                    }
                }
                info.write(InfoMessage {
                    target: actor,
                    text,
                });
            }
        }
    }
}

/// One disambiguation line for a room: `Name (entity:… grim:… slug:…)`. Matches
/// the admin room-title debug format. (A future instance id would slot in here.)
fn room_ident_line(entity: Entity, room: &Room) -> String {
    format!(
        "{} (entity:{} grim:{} slug:{})",
        room.name,
        entity.to_bits(),
        room.id,
        room.friendly_id
    )
}

/// Wire the `move` and `goto` handlers and the input/delivery messages they
/// own. The world-happening events they emit (`MoveEvent`/`LookRoom`) are
/// registered by `grim_world::WorldPlugin`.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_systems(Update, (handle_move, handle_goto));
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim_engine_types::cardinal::Cardinal;
    use grim_engine_types::character::Gender;
    // The project display-name component, aliased to dodge Bevy's prelude `Name`
    // (the glob above brings Bevy's in scope). See AGENTS.md.
    use grim_engine_types::components::Name as GrimName;
    use grim_engine_types::GrimId;

    use crate::actor::Actor;
    use crate::character::Role;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(grim_world::WorldPlugin);
        register(&mut app);
        app
    }

    fn look_room_count(app: &App) -> usize {
        let messages = app.world().resource::<Messages<LookRoom>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).count()
    }

    /// Spawn an area + a room in it, returning the room entity. `friendly_id`s
    /// are the stable storage keys `last_room` records.
    fn spawn_room(app: &mut App, area_fid: &str, room_fid: &str, exits: Exits) -> Entity {
        let area = app
            .world_mut()
            .spawn(Area {
                id: GrimId::new(),
                friendly_id: area_fid.into(),
                name: area_fid.into(),
            })
            .id();
        app.world_mut()
            .spawn((
                Room {
                    id: GrimId::new(),
                    friendly_id: room_fid.into(),
                    name: room_fid.into(),
                    description: String::new(),
                    area,
                },
                exits,
            ))
            .id()
    }

    fn spawn_actor_in(app: &mut App, room: Entity, admin: bool) -> Entity {
        let roles = if admin { vec![Role::Admin] } else { Vec::new() };
        app.world_mut()
            .spawn((
                InRoom { room },
                GrimName("Admin".into()),
                Actor {
                    race: String::new(),
                    level: 1,
                    gender: Gender::Neutral,
                },
                Character {
                    id: GrimId::new(),
                    account_id: GrimId::new(),
                    created_at: chrono::Utc::now(),
                    last_room: None,
                    roles,
                    class: String::new(),
                    title: None,
                    restrings: std::collections::HashMap::new(),
                },
            ))
            .id()
    }

    fn send_goto(app: &mut App, actor: Entity, target: &str) {
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Goto {
                target: target.into(),
            },
        });
        app.update();
    }

    fn info_texts(app: &App) -> Vec<String> {
        let messages = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|m| m.text.clone()).collect()
    }

    fn room_of(app: &App, actor: Entity) -> Entity {
        app.world().get::<InRoom>(actor).unwrap().room
    }

    // ── walking an exit ──────────────────────────────────────────────
    mod walking {
        use super::*;

        #[test]
        fn move_valid_exit_updates_in_room() {
            let mut app = test_app();
            let room2 = app.world_mut().spawn(()).id();
            let mut exits = Exits::default();
            exits.exits.insert(Cardinal::North, room2);
            let room1 = app.world_mut().spawn(exits).id();
            let actor = app.world_mut().spawn(InRoom { room: room1 }).id();
            app.world_mut().write_message(EngineCommand {
                client: actor,
                command: Command::Move {
                    direction: Cardinal::North,
                },
            });
            app.update();
            assert_eq!(app.world().get::<InRoom>(actor).unwrap().room, room2);
            {
                let messages = app.world().resource::<Messages<MoveEvent>>();
                let mut cursor = messages.get_cursor();
                let mut iter = cursor.read(messages);
                let ev = iter.next().expect("expected one MoveEvent");
                assert_eq!(ev.from, room1);
                assert_eq!(ev.to, room2);
                assert!(iter.next().is_none(), "expected exactly one MoveEvent");
            }
            assert_eq!(look_room_count(&app), 1);
        }

        #[test]
        fn move_no_exit_emits_info_message() {
            let mut app = test_app();
            let room1 = app.world_mut().spawn(Exits::default()).id();
            let actor = app.world_mut().spawn(InRoom { room: room1 }).id();
            app.world_mut().write_message(EngineCommand {
                client: actor,
                command: Command::Move {
                    direction: Cardinal::North,
                },
            });
            app.update();
            assert_eq!(app.world().get::<InRoom>(actor).unwrap().room, room1);
            {
                let messages = app.world().resource::<Messages<InfoMessage>>();
                let mut cursor = messages.get_cursor();
                let mut iter = cursor.read(messages);
                let ev = iter.next().expect("expected one InfoMessage");
                assert_eq!(ev.text, "You can't go that way.\n");
                assert!(iter.next().is_none(), "expected exactly one InfoMessage");
            }
        }

        #[test]
        fn move_updates_character_last_room_to_destination_friendly_ids() {
            let mut app = test_app();
            let room2 = spawn_room(&mut app, "town", "market", Exits::default());
            let mut exits = Exits::default();
            exits.exits.insert(Cardinal::North, room2);
            let room1 = spawn_room(&mut app, "town", "square", exits);
            let actor = app
                .world_mut()
                .spawn((
                    InRoom { room: room1 },
                    GrimName("Walker".into()),
                    Actor {
                        race: String::new(),
                        level: 1,
                        gender: Gender::Neutral,
                    },
                    Character {
                        id: GrimId::new(),
                        account_id: GrimId::new(),
                        created_at: chrono::Utc::now(),
                        last_room: None,
                        roles: Vec::new(),
                        class: String::new(),
                        title: None,
                        restrings: std::collections::HashMap::new(),
                    },
                ))
                .id();
            app.world_mut().write_message(EngineCommand {
                client: actor,
                command: Command::Move {
                    direction: Cardinal::North,
                },
            });
            app.update();
            let loc = app
                .world()
                .get::<Character>(actor)
                .unwrap()
                .last_room
                .clone()
                .expect("last_room should be set after moving");
            assert_eq!(loc.area, "town");
            assert_eq!(loc.room, "market");
        }

        #[test]
        fn move_without_in_room_is_ignored() {
            // An actor with no `InRoom` placement can't move; the handler skips
            // it rather than erroring.
            let mut app = test_app();
            let actor = app.world_mut().spawn(()).id();
            app.world_mut().write_message(EngineCommand {
                client: actor,
                command: Command::Move {
                    direction: Cardinal::North,
                },
            });
            app.update();
            let messages = app.world().resource::<Messages<InfoMessage>>();
            let mut cursor = messages.get_cursor();
            assert_eq!(cursor.read(messages).count(), 0);
            assert_eq!(look_room_count(&app), 0);
        }

        #[test]
        fn move_from_room_without_exits_component_blocks() {
            // The current room has no `Exits` component at all → the exits query
            // errors and the move is refused with "You can't go that way".
            let mut app = test_app();
            let room = app.world_mut().spawn(()).id();
            let actor = app.world_mut().spawn(InRoom { room }).id();
            app.world_mut().write_message(EngineCommand {
                client: actor,
                command: Command::Move {
                    direction: Cardinal::North,
                },
            });
            app.update();
            assert_eq!(app.world().get::<InRoom>(actor).unwrap().room, room);
            let messages = app.world().resource::<Messages<InfoMessage>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            assert_eq!(iter.next().unwrap().text, "You can't go that way.\n");
            assert!(iter.next().is_none());
        }

        #[test]
        fn move_without_character_component_does_not_panic() {
            // A non-character actor (e.g. an NPC) can move; the last_room update
            // is simply skipped rather than erroring.
            let mut app = test_app();
            let room2 = spawn_room(&mut app, "town", "market", Exits::default());
            let mut exits = Exits::default();
            exits.exits.insert(Cardinal::North, room2);
            let room1 = spawn_room(&mut app, "town", "square", exits);
            let actor = app.world_mut().spawn(InRoom { room: room1 }).id();
            app.world_mut().write_message(EngineCommand {
                client: actor,
                command: Command::Move {
                    direction: Cardinal::North,
                },
            });
            app.update();
            assert_eq!(app.world().get::<InRoom>(actor).unwrap().room, room2);
        }
    }

    // ── admin goto + address resolution ──────────────────────────────
    mod goto {
        use super::*;

        #[test]
        fn goto_bare_slug_moves_admin_and_updates_last_room() {
            let mut app = test_app();
            let dest = spawn_room(&mut app, "town", "market", Exits::default());
            let start = spawn_room(&mut app, "town", "square", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);
            send_goto(&mut app, actor, "market");
            assert_eq!(room_of(&app, actor), dest);
            assert_eq!(look_room_count(&app), 1);
            let loc = app
                .world()
                .get::<Character>(actor)
                .unwrap()
                .last_room
                .clone()
                .expect("goto should refresh last_room");
            assert_eq!((loc.area.as_str(), loc.room.as_str()), ("town", "market"));
        }

        #[test]
        fn goto_area_room_slug_disambiguates() {
            let mut app = test_app();
            let _town_market = spawn_room(&mut app, "town", "market", Exits::default());
            let forest_market = spawn_room(&mut app, "forest", "market", Exits::default());
            let start = spawn_room(&mut app, "town", "square", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);
            send_goto(&mut app, actor, "forest:market");
            assert_eq!(room_of(&app, actor), forest_market);
        }

        #[test]
        fn goto_bare_slug_matching_two_areas_lists_candidates() {
            let mut app = test_app();
            spawn_room(&mut app, "town", "market", Exits::default());
            spawn_room(&mut app, "forest", "market", Exits::default());
            let start = spawn_room(&mut app, "town", "square", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);
            send_goto(&mut app, actor, "market");
            assert_eq!(room_of(&app, actor), start, "ambiguous goto must not move");
            let text = info_texts(&app).join("");
            assert!(text.contains("Select an option..."));
            // One detail line per candidate, each carrying its ids.
            let lines: Vec<&str> = text.lines().filter(|l| l.contains("entity:")).collect();
            assert_eq!(lines.len(), 2, "expected two candidates, got:\n{text}");
            assert!(lines
                .iter()
                .all(|l| l.contains("grim:") && l.contains("slug:market")));
        }

        #[test]
        fn goto_area_room_all_token_permutations_resolve() {
            // area side ∈ {entity, grim, slug} × room side ∈ {entity, grim, slug}.
            let mut app = test_app();
            let market = spawn_room(&mut app, "town", "market", Exits::default());
            let start = spawn_room(&mut app, "forest", "clearing", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);

            let area = app.world().get::<Room>(market).unwrap().area;
            let area_tokens = [
                area.to_bits().to_string(),
                app.world().get::<Area>(area).unwrap().id.to_string(),
                "town".to_string(),
            ];
            let room_tokens = [
                market.to_bits().to_string(),
                app.world().get::<Room>(market).unwrap().id.to_string(),
                "market".to_string(),
            ];
            for a in &area_tokens {
                for r in &room_tokens {
                    app.world_mut().get_mut::<InRoom>(actor).unwrap().room = start;
                    send_goto(&mut app, actor, &format!("{a}:{r}"));
                    assert_eq!(
                        room_of(&app, actor),
                        market,
                        "address {a}:{r} should resolve"
                    );
                }
            }
        }

        #[test]
        fn goto_bare_all_token_forms_resolve() {
            // bare room token ∈ {entity, grim, slug}.
            let mut app = test_app();
            let market = spawn_room(&mut app, "town", "market", Exits::default());
            let start = spawn_room(&mut app, "town", "square", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);
            let tokens = [
                market.to_bits().to_string(),
                app.world().get::<Room>(market).unwrap().id.to_string(),
                "market".to_string(),
            ];
            for t in &tokens {
                app.world_mut().get_mut::<InRoom>(actor).unwrap().room = start;
                send_goto(&mut app, actor, t);
                assert_eq!(
                    room_of(&app, actor),
                    market,
                    "bare token {t} should resolve"
                );
            }
        }

        #[test]
        fn goto_area_room_slug_ambiguous_within_area_lists_candidates() {
            // Two rooms sharing a slug in the SAME area (e.g. instanced) → the
            // `<area>:<room>` slug path is itself ambiguous and lists both.
            let mut app = test_app();
            let a = spawn_room(&mut app, "town", "market", Exits::default());
            let area = app.world().get::<Room>(a).unwrap().area;
            // A second "market" room in the very same area.
            app.world_mut().spawn((
                Room {
                    id: GrimId::new(),
                    friendly_id: "market".into(),
                    name: "Town Square".into(),
                    description: String::new(),
                    area,
                },
                GrimName("Town Square".into()),
                Exits::default(),
            ));
            let start = spawn_room(&mut app, "forest", "clearing", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);
            send_goto(&mut app, actor, "town:market");
            assert_eq!(
                room_of(&app, actor),
                start,
                "ambiguous area:room must not move"
            );
            let text = info_texts(&app).join("");
            assert!(text.contains("Select an option..."));
            assert_eq!(text.lines().filter(|l| l.contains("entity:")).count(), 2);
        }

        #[test]
        fn goto_area_room_not_found_variants() {
            let mut app = test_app();
            spawn_room(&mut app, "town", "market", Exits::default());
            let start = spawn_room(&mut app, "town", "square", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);
            for addr in ["nowhere:market", "town:nowhere"] {
                app.world_mut().get_mut::<InRoom>(actor).unwrap().room = start;
                send_goto(&mut app, actor, addr);
                assert_eq!(room_of(&app, actor), start, "{addr} must not move");
            }
            assert!(info_texts(&app)
                .iter()
                .any(|t| t.contains("No room matches")));
        }

        #[test]
        fn goto_by_entity_id_moves() {
            let mut app = test_app();
            let dest = spawn_room(&mut app, "town", "market", Exits::default());
            let start = spawn_room(&mut app, "town", "square", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);
            send_goto(&mut app, actor, &dest.to_bits().to_string());
            assert_eq!(room_of(&app, actor), dest);
        }

        #[test]
        fn goto_by_room_grim_id_moves() {
            let mut app = test_app();
            let dest = spawn_room(&mut app, "town", "market", Exits::default());
            let start = spawn_room(&mut app, "town", "square", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);
            let gid = app.world().get::<Room>(dest).unwrap().id.to_string();
            send_goto(&mut app, actor, &gid);
            assert_eq!(room_of(&app, actor), dest);
        }

        #[test]
        fn goto_by_area_grim_id_and_room_slug() {
            let mut app = test_app();
            let dest = spawn_room(&mut app, "town", "market", Exits::default());
            let start = spawn_room(&mut app, "forest", "clearing", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);
            let area_gid = {
                let area = app.world().get::<Room>(dest).unwrap().area;
                app.world().get::<Area>(area).unwrap().id.to_string()
            };
            send_goto(&mut app, actor, &format!("{area_gid}:market"));
            assert_eq!(room_of(&app, actor), dest);
        }

        #[test]
        fn goto_unknown_slug_reports_not_found() {
            let mut app = test_app();
            let start = spawn_room(&mut app, "town", "square", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);
            send_goto(&mut app, actor, "nowhere");
            assert_eq!(room_of(&app, actor), start);
            assert!(info_texts(&app)
                .iter()
                .any(|t| t.contains("No room matches 'nowhere'")));
        }

        #[test]
        fn goto_numeric_that_is_not_a_live_room_falls_through() {
            // A well-formed but dead entity id parses, misses the live-room check,
            // and falls through to the (also-missing) slug lookup → NotFound.
            let mut app = test_app();
            let start = spawn_room(&mut app, "town", "square", Exits::default());
            let ghost = app.world_mut().spawn(()).id();
            let bits = ghost.to_bits();
            app.world_mut().despawn(ghost);
            let actor = spawn_actor_in(&mut app, start, true);
            send_goto(&mut app, actor, &bits.to_string());
            assert_eq!(room_of(&app, actor), start);
            assert!(info_texts(&app)
                .iter()
                .any(|t| t.contains("No room matches")));
        }

        #[test]
        fn goto_empty_target_reports_not_found() {
            let mut app = test_app();
            let start = spawn_room(&mut app, "town", "square", Exits::default());
            let actor = spawn_actor_in(&mut app, start, true);
            send_goto(&mut app, actor, "");
            assert_eq!(room_of(&app, actor), start);
            assert!(!info_texts(&app).is_empty());
        }

        #[test]
        fn goto_is_ignored_for_non_admin() {
            let mut app = test_app();
            let dest = spawn_room(&mut app, "town", "market", Exits::default());
            let start = spawn_room(&mut app, "town", "square", Exits::default());
            let _ = dest;
            let actor = spawn_actor_in(&mut app, start, false);
            send_goto(&mut app, actor, "market");
            assert_eq!(room_of(&app, actor), start, "non-admin goto must not move");
            assert_eq!(look_room_count(&app), 0);
            assert!(
                info_texts(&app).is_empty(),
                "non-admin goto must stay silent"
            );
        }
    }
}
