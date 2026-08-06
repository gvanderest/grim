//! World subsystem plugin: wires the `look`, `move`, `goto`, and `quit` command
//! handlers and the world messages they emit. The concern logic lives in the
//! sibling modules — [`look`] (rendering), [`movement`] (`move`/`goto` + the
//! shared placement seam), and [`area`] (rooms/areas/exits + address lookup).

use bevy::prelude::*;
use grim_engine_types::components::Player;
use grim_engine_types::events::{
    Command, EngineCommand, InfoMessage, LookEntity, LookRoom, MoveEvent,
};
use grim_networking::DisconnectRequest;

mod area;
mod look;
mod movement;
mod title;
mod topology;

// Preserve the pre-split public paths (`grim_world::world::*`): these lookups
// were public here before the concern modules were carved out.
pub use area::{resolve_room_address, room_location, RoomLookup};
// World topology types (Placement Phase 2a): re-exported at `grim_world::world::*`
// and hoisted to the crate root in `lib.rs`.
pub use topology::{Area, Exits, Room, StartingRoom};

/// Handles `look`, `move`, and `quit` commands; emits room/entity description
/// events, movement broadcasts, and disconnect requests.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EngineCommand>()
            .add_message::<LookRoom>()
            .add_message::<LookEntity>()
            .add_message::<MoveEvent>()
            .add_message::<InfoMessage>()
            .add_message::<DisconnectRequest>()
            .add_systems(
                Update,
                (
                    look::handle_look,
                    movement::handle_move,
                    handle_quit,
                    movement::handle_goto,
                    title::handle_title,
                ),
            );
    }
}

/// `quit`: request a clean disconnect of the actor's underlying connection.
fn handle_quit(
    mut engine: MessageReader<EngineCommand>,
    players: Query<&Player>,
    mut disconnect: MessageWriter<DisconnectRequest>,
) {
    for cmd in engine.read() {
        let Command::Quit = cmd.command else {
            continue;
        };
        if let Ok(player) = players.get(cmd.client) {
            if let Some(conn) = player.connection {
                disconnect.write(DisconnectRequest { connection: conn });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::topology::{Area, Exits, Room};
    use super::*;
    use grim_engine_types::cardinal::Cardinal;
    use grim_engine_types::components::{Character, Gender, InRoom, Name, Role};
    use grim_engine_types::events::{
        Command, EngineCommand, InfoMessage, LookEntity, LookRoom, MoveEvent,
    };
    use grim_engine_types::GrimId;

    macro_rules! count_messages {
        ($app:expr, $t:ty) => {{
            let messages = $app.world().resource::<Messages<$t>>();
            let mut cursor = messages.get_cursor();
            cursor.read(messages).count()
        }};
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(WorldPlugin);
        app
    }

    fn look_room_count(app: &App) -> usize {
        count_messages!(app, LookRoom)
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
                Character {
                    id: GrimId::new(),
                    name: "Admin".into(),
                    account_id: GrimId::new(),
                    created_at: chrono::Utc::now(),
                    last_room: None,
                    roles,
                    gender: Gender::Neutral,
                    race: String::new(),
                    class: String::new(),
                    level: 1,
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

    // ── look rendering ───────────────────────────────────────────────
    mod look_rendering {
        use super::*;

        #[test]
        fn look_no_target_emits_look_room() {
            let mut app = test_app();
            let room = app.world_mut().spawn(()).id();
            let actor = app.world_mut().spawn(InRoom { room }).id();
            app.world_mut().write_message(EngineCommand {
                client: actor,
                command: Command::Look { target: None },
            });
            app.update();
            {
                let messages = app.world().resource::<Messages<LookRoom>>();
                let mut cursor = messages.get_cursor();
                let mut iter = cursor.read(messages);
                let ev = iter.next().expect("expected one LookRoom");
                assert_eq!(ev.target, actor);
                assert_eq!(ev.room, room);
                assert!(iter.next().is_none(), "expected exactly one LookRoom");
            }
        }

        #[test]
        fn look_valid_target_emits_look_entity() {
            let mut app = test_app();
            let room = app.world_mut().spawn(()).id();
            let actor = app
                .world_mut()
                .spawn((InRoom { room }, Name("hero".into())))
                .id();
            let goblin = app
                .world_mut()
                .spawn((InRoom { room }, Name("goblin".into())))
                .id();
            app.world_mut().write_message(EngineCommand {
                client: actor,
                command: Command::Look {
                    target: Some("goblin".into()),
                },
            });
            app.update();
            {
                let messages = app.world().resource::<Messages<LookEntity>>();
                let mut cursor = messages.get_cursor();
                let mut iter = cursor.read(messages);
                let ev = iter.next().expect("expected one LookEntity");
                assert_eq!(ev.target, actor);
                assert_eq!(ev.subject, goblin);
                assert!(iter.next().is_none(), "expected exactly one LookEntity");
            }
        }

        #[test]
        fn look_invalid_target_emits_info_message() {
            let mut app = test_app();
            let room = app.world_mut().spawn(()).id();
            let actor = app
                .world_mut()
                .spawn((InRoom { room }, Name("hero".into())))
                .id();
            app.world_mut().write_message(EngineCommand {
                client: actor,
                command: Command::Look {
                    target: Some("ghost".into()),
                },
            });
            app.update();
            {
                let messages = app.world().resource::<Messages<InfoMessage>>();
                let mut cursor = messages.get_cursor();
                let mut iter = cursor.read(messages);
                let ev = iter.next().expect("expected one InfoMessage");
                assert_eq!(ev.target, actor);
                assert_eq!(ev.text, "You don't see that here.\n");
                assert!(iter.next().is_none(), "expected exactly one InfoMessage");
            }
            assert_eq!(look_room_count(&app), 0);
        }
    }

    // ── movement ─────────────────────────────────────────────────────
    mod movement {
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
                    Character {
                        id: GrimId::new(),
                        name: "Walker".into(),
                        account_id: GrimId::new(),
                        created_at: chrono::Utc::now(),
                        last_room: None,
                        roles: Vec::new(),
                        gender: Gender::Neutral,
                        race: String::new(),
                        class: String::new(),
                        level: 1,
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

    // ── rooms / areas / exits: goto + address resolution ─────────────
    mod rooms_areas_exits {
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
                Name("Town Square".into()),
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
