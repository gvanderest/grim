use bevy::prelude::*;
use grim_engine_types::components::{
    Area, Character, Exits, InRoom, Name, Player, Room, RoomLocation,
};
use grim_engine_types::events::{
    Command, EngineCommand, InfoMessage, LookEntity, LookRoom, MoveEvent,
};
use grim_networking::DisconnectRequest;

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
            .add_systems(Update, (handle_look, handle_move, handle_quit, handle_goto));
    }
}

/// `look` / `look <target>`: show the actor's room or a named entity in it.
fn handle_look(
    mut engine: MessageReader<EngineCommand>,
    inroom: Query<&InRoom>,
    named: Query<(Entity, &InRoom, &Name)>,
    mut look_room: MessageWriter<LookRoom>,
    mut look_entity: MessageWriter<LookEntity>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Look { target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok(actor_room) = inroom.get(actor) else {
            continue;
        };
        match target {
            None => {
                look_room.write(LookRoom {
                    target: actor,
                    room: actor_room.room,
                });
            }
            Some(name) => {
                let want = name.to_lowercase();
                let room = actor_room.room;
                let subject = named
                    .iter()
                    .find(|(_, ir, nm)| ir.room == room && nm.0.to_lowercase() == want)
                    .map(|(e, _, _)| e);
                match subject {
                    Some(subject) => {
                        look_entity.write(LookEntity {
                            target: actor,
                            subject,
                        });
                    }
                    None => {
                        info.write(InfoMessage {
                            target: actor,
                            text: "You don't see that here.\n".into(),
                        });
                    }
                }
            }
        }
    }
}

/// Resolve a room entity to its stable, entity-independent storage location
/// (area + room `friendly_id`s). These survive a world reseed, so persisting
/// them lets a character be placed back into the *new* instance of the same room
/// after a restart or copyover — see `grim-scene`'s placement resolver.
pub fn room_location(
    room: Entity,
    rooms: &Query<&Room>,
    areas: &Query<&Area>,
) -> Option<RoomLocation> {
    let r = rooms.get(room).ok()?;
    let area = areas.get(r.area).ok()?;
    Some(RoomLocation {
        area: area.friendly_id.clone(),
        room: r.friendly_id.clone(),
    })
}

/// Outcome of resolving a room [address](resolve_room_address).
#[derive(Debug, PartialEq, Eq)]
pub enum RoomLookup {
    /// Exactly one room matched.
    Found(Entity),
    /// Nothing matched the address.
    NotFound,
    /// A slug matched more than one room (e.g. several instances of an area).
    /// Carries every candidate so the caller can list them for disambiguation.
    Ambiguous(Vec<Entity>),
}

/// Resolve a room *address* to a room entity — the shared lookup behind admin
/// `goto` and (later) other targeting. See `docs/adr/0001`.
///
/// Precedence, most specific first: an **entity id** (`Entity::to_bits` as a
/// decimal, boot-local), then a **grim id** (globally unique), then a **slug**
/// (`friendly_id`). An address is either `<area>:<room>` — each side
/// independently an entity id, grim id, or slug — or a bare room token. A bare
/// slug that matches rooms in several areas is [`Ambiguous`](RoomLookup::Ambiguous)
/// (grim ids never are).
pub fn resolve_room_address(
    input: &str,
    rooms: &Query<(Entity, &Room)>,
    areas: &Query<(Entity, &Area)>,
) -> RoomLookup {
    let input = input.trim();
    if input.is_empty() {
        return RoomLookup::NotFound;
    }

    if let Some((area_tok, room_tok)) = input.split_once(':') {
        let Some(area) = resolve_area(area_tok.trim(), areas) else {
            return RoomLookup::NotFound;
        };
        return resolve_room_in_area(room_tok.trim(), area, rooms);
    }

    // Bare token. Entity id is most specific.
    if let Some(e) = parse_entity(input) {
        if rooms.get(e).is_ok() {
            return RoomLookup::Found(e);
        }
        // A numeric token that isn't a live room falls through — a grim id
        // could in principle be all digits — rather than hard-failing.
    }

    // Grim ID: globally unique, so an exact match wins outright.
    if let Some((e, _)) = rooms.iter().find(|(_, r)| r.id.as_str() == input) {
        return RoomLookup::Found(e);
    }

    // Slug: a room `friendly_id`, which is unique only within its area.
    let hits: Vec<Entity> = rooms
        .iter()
        .filter(|(_, r)| r.friendly_id.eq_ignore_ascii_case(input))
        .map(|(e, _)| e)
        .collect();
    classify(hits)
}

/// Turn a list of slug candidates into a lookup outcome.
fn classify(mut hits: Vec<Entity>) -> RoomLookup {
    match hits.len() {
        0 => RoomLookup::NotFound,
        1 => RoomLookup::Found(hits.remove(0)),
        _ => RoomLookup::Ambiguous(hits),
    }
}

/// Resolve the area side of an `<area>:<room>` address to an area entity.
fn resolve_area(tok: &str, areas: &Query<(Entity, &Area)>) -> Option<Entity> {
    if let Some(e) = parse_entity(tok) {
        if areas.get(e).is_ok() {
            return Some(e);
        }
    }
    // Grim id (globally unique) before slug.
    if let Some((e, _)) = areas.iter().find(|(_, a)| a.id.as_str() == tok) {
        return Some(e);
    }
    areas
        .iter()
        .find(|(_, a)| a.friendly_id.eq_ignore_ascii_case(tok))
        .map(|(e, _)| e)
}

/// Resolve the room side of an `<area>:<room>` address within a known area.
fn resolve_room_in_area(tok: &str, area: Entity, rooms: &Query<(Entity, &Room)>) -> RoomLookup {
    if let Some(e) = parse_entity(tok) {
        if rooms.get(e).map(|(_, r)| r.area == area).unwrap_or(false) {
            return RoomLookup::Found(e);
        }
    }
    // Grim id (globally unique) before slug.
    if let Some((e, _)) = rooms
        .iter()
        .find(|(_, r)| r.area == area && r.id.as_str() == tok)
    {
        return RoomLookup::Found(e);
    }
    // Slug within the area — may match multiple instances of the same room.
    let hits: Vec<Entity> = rooms
        .iter()
        .filter(|(_, r)| r.area == area && r.friendly_id.eq_ignore_ascii_case(tok))
        .map(|(e, _)| e)
        .collect();
    classify(hits)
}

/// Parse an entity-id token (`Entity::to_bits` decimal) into a well-formed
/// entity, or `None` if it is not a valid bit pattern. Whether it is *live* is
/// the caller's check.
fn parse_entity(tok: &str) -> Option<Entity> {
    tok.parse::<u64>().ok().and_then(Entity::try_from_bits)
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
fn handle_move(
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
fn handle_goto(
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
                    areas.get(r.area).ok().map(|(_, a)| RoomLocation {
                        area: a.friendly_id.clone(),
                        room: r.friendly_id.clone(),
                    })
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
    use super::*;
    use grim_engine_types::cardinal::Cardinal;
    use grim_engine_types::components::{Exits, InRoom, Name};
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

    /// Spawn an area + a room in it, returning the room entity. `friendly_id`s
    /// are the stable storage keys `last_room` records.
    fn spawn_room(app: &mut App, area_fid: &str, room_fid: &str, exits: Exits) -> Entity {
        use grim_engine_types::components::{Area, Room};
        use grim_engine_types::GrimId;
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

    #[test]
    fn move_updates_character_last_room_to_destination_friendly_ids() {
        use grim_engine_types::components::Character;
        use grim_engine_types::GrimId;
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
        // A non-character actor (e.g. an NPC) can move; the last_room update is
        // simply skipped rather than erroring.
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

    // ── goto / resolve_room_address ──────────────────────────────────

    fn spawn_actor_in(app: &mut App, room: Entity, admin: bool) -> Entity {
        use grim_engine_types::components::{Character, Role};
        use grim_engine_types::GrimId;
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

    #[test]
    fn goto_bare_slug_moves_admin_and_updates_last_room() {
        use grim_engine_types::components::Character;
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
