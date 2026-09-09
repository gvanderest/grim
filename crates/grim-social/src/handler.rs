//! The social handler: resolve `Command::Social` against the actor's
//! roommates and render per-recipient [`InfoMessage`]s — the actor's wording,
//! the target's (when there is one), and the room's. The same primitive `tell`
//! uses; the future `act()` extraction (#58) absorbs this fan-out.

use bevy::prelude::*;
use grim_actor::InRoom;
use grim_core::components::Name;
use grim_core::events::{Command, EngineCommand, InfoMessage};
use grim_target::rank_match;

use crate::social::SocialRegistry;

/// A social was performed and rendered. A fact for logging/moderation
/// observers; nothing reads it yet. A future mute/gag veto would hang its
/// attempt off this point.
#[derive(Message, Debug)]
pub struct SocialPerformed {
    pub actor: Entity,
    pub name: String,
    pub target: Option<Entity>,
}

pub(crate) fn handle_social(
    mut engine: MessageReader<EngineCommand>,
    registry: Res<SocialRegistry>,
    inroom: Query<&InRoom>,
    occupants: Query<(Entity, &InRoom, &Name)>,
    names: Query<&Name>,
    mut info: MessageWriter<InfoMessage>,
    mut performed: MessageWriter<SocialPerformed>,
) {
    for cmd in engine.read() {
        let Command::Social { name, target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Some(def) = registry.get(name) else {
            send(
                &mut info,
                actor,
                &grim_text::tr("error.unknown_command", &[]),
            );
            continue;
        };
        let Ok(actor_room) = inroom.get(actor).map(|ir| ir.room) else {
            continue;
        };
        let Ok(actor_name) = names.get(actor).map(|n| n.0.clone()) else {
            continue;
        };
        let here = roommates(&occupants, actor_room);
        let word = target.as_deref().and_then(|t| t.split_whitespace().next());
        let Some(word) = word else {
            emit_solo(&mut info, def, actor, &actor_name, &here);
            performed.write(SocialPerformed {
                actor,
                name: name.clone(),
                target: None,
            });
            continue;
        };
        let Some(recipient) = resolve_target(&here, word, actor) else {
            send(
                &mut info,
                actor,
                &grim_text::tr("social.error.unknown_target", &[]),
            );
            continue;
        };
        if recipient == actor {
            emit_self(&mut info, def, actor, &actor_name, &here);
        } else {
            let target_name = names
                .get(recipient)
                .map(|n| n.0.clone())
                .unwrap_or_default();
            emit_other(
                &mut info,
                def,
                actor,
                &actor_name,
                recipient,
                &target_name,
                &here,
            );
        }
        performed.write(SocialPerformed {
            actor,
            name: name.clone(),
            target: Some(recipient),
        });
    }
}

/// Everyone standing in `room`: entity + display name.
fn roommates(occupants: &Query<(Entity, &InRoom, &Name)>, room: Entity) -> Vec<(Entity, String)> {
    occupants
        .iter()
        .filter(|(_, ir, _)| ir.room == room)
        .map(|(e, _, n)| (e, n.0.clone()))
        .collect()
}

/// `self` names the actor; otherwise rank-match the word among roommates
/// (exact beats prefix, ties to the lowest entity — the `tell` rule).
fn resolve_target(here: &[(Entity, String)], word: &str, actor: Entity) -> Option<Entity> {
    if word.eq_ignore_ascii_case("self") {
        return Some(actor);
    }
    let want = word.to_lowercase();
    here.iter()
        .filter_map(|(e, n)| {
            rank_match(n, &[], std::slice::from_ref(&want)).map(|rank| (rank, e.to_bits(), *e))
        })
        .min()
        .map(|(_, _, e)| e)
}

/// Solo: the actor's wording to them, the room's to everyone else here.
fn emit_solo(
    info: &mut MessageWriter<InfoMessage>,
    def: &crate::SocialDef,
    actor: Entity,
    actor_name: &str,
    here: &[(Entity, String)],
) {
    send(info, actor, &render(def, "solo", "actor", actor_name, ""));
    for (entity, _) in here {
        if *entity != actor {
            send(info, *entity, &render(def, "solo", "room", actor_name, ""));
        }
    }
}

/// Self-targeted: the self case, with the same actor/room split.
fn emit_self(
    info: &mut MessageWriter<InfoMessage>,
    def: &crate::SocialDef,
    actor: Entity,
    actor_name: &str,
    here: &[(Entity, String)],
) {
    send(info, actor, &render(def, "self", "actor", actor_name, ""));
    for (entity, _) in here {
        if *entity != actor {
            send(info, *entity, &render(def, "self", "room", actor_name, ""));
        }
    }
}

/// Targeted at another: actor, target, and room wordings.
fn emit_other(
    info: &mut MessageWriter<InfoMessage>,
    def: &crate::SocialDef,
    actor: Entity,
    actor_name: &str,
    recipient: Entity,
    target_name: &str,
    here: &[(Entity, String)],
) {
    send(
        info,
        actor,
        &render(def, "other", "actor", actor_name, target_name),
    );
    send(
        info,
        recipient,
        &render(def, "other", "target", actor_name, target_name),
    );
    for (entity, _) in here {
        if *entity != actor && *entity != recipient {
            send(
                info,
                *entity,
                &render(def, "other", "room", actor_name, target_name),
            );
        }
    }
}

fn render(def: &crate::SocialDef, case: &str, audience: &str, actor: &str, target: &str) -> String {
    grim_text::render(
        &def.template(case, audience),
        &[("actor", actor), ("target", target)],
    )
}

fn send(info: &mut MessageWriter<InfoMessage>, target: Entity, text: &str) {
    info.write(InfoMessage {
        target,
        text: text.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim_actor::{Actor, Character, Player};
    use grim_command::CommandRegistry;
    use grim_core::character::Gender;
    use grim_core::events::Command as Cmd;
    use grim_core::GrimId;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<EngineCommand>();
        app.add_message::<InfoMessage>();
        app.add_message::<SocialPerformed>();
        app.insert_resource(SocialRegistry::default());
        app.add_systems(Update, handle_social);
        app
    }

    fn character() -> Character {
        Character {
            id: GrimId::new(),
            account_id: GrimId::new(),
            created_at: chrono::Utc::now(),
            last_room: None,
            roles: Vec::new(),
            class: String::new(),
            title: None,
            restrings: std::collections::HashMap::new(),
        }
    }

    fn spawn_being(app: &mut App, name: &str, room: Entity) -> Entity {
        app.world_mut()
            .spawn((
                Name(name.into()),
                Actor {
                    race: "human".into(),
                    level: 1,
                    gender: Gender::Neutral,
                },
                character(),
                Player {
                    connection: Entity::PLACEHOLDER,
                },
                InRoom { room },
            ))
            .id()
    }

    /// Two rooms: Alice/Bob/Carol together, Dave elsewhere.
    fn setup() -> (App, Entity, Entity, Entity) {
        let mut app = test_app();
        let room = app.world_mut().spawn_empty().id();
        let elsewhere = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<SocialRegistry>()
            .insert(crate::SocialDef::new("grin"));
        let alice = spawn_being(&mut app, "Alice", room);
        let bob = spawn_being(&mut app, "Bob", room);
        let carol = spawn_being(&mut app, "Carol", room);
        let _dave = spawn_being(&mut app, "Dave", elsewhere);
        (app, alice, bob, carol)
    }

    fn emit(app: &mut App, actor: Entity, name: &str, target: Option<&str>) {
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Cmd::Social {
                name: name.into(),
                target: target.map(str::to_string),
            },
        });
        app.update();
    }

    fn infos(app: &App) -> Vec<(Entity, String)> {
        let m = app.world().resource::<Messages<InfoMessage>>();
        let mut c = m.get_cursor();
        c.read(m).map(|i| (i.target, i.text.clone())).collect()
    }

    fn by_target(app: &App, target: Entity) -> Vec<String> {
        infos(app)
            .into_iter()
            .filter(|(e, _)| *e == target)
            .map(|(_, t)| t)
            .collect()
    }

    #[test]
    fn solo_reaches_actor_and_room_only() {
        let (mut app, alice, bob, carol) = setup();
        emit(&mut app, alice, "grin", None);
        assert_eq!(by_target(&app, alice), vec!["You grin.\n"]);
        assert_eq!(by_target(&app, bob), vec!["Alice grins.\n"]);
        assert_eq!(by_target(&app, carol), vec!["Alice grins.\n"]);
        // Dave is in another room: nothing. Three messages total.
        assert_eq!(infos(&app).len(), 3);
    }

    #[test]
    fn other_target_reaches_all_three_audiences() {
        let (mut app, alice, bob, carol) = setup();
        emit(&mut app, alice, "grin", Some("bob"));
        assert_eq!(by_target(&app, alice), vec!["You grin at Bob.\n"]);
        assert_eq!(by_target(&app, bob), vec!["Alice grins at you.\n"]);
        assert_eq!(by_target(&app, carol), vec!["Alice grins at Bob.\n"]);
    }

    #[test]
    fn self_keyword_uses_self_case() {
        let (mut app, alice, bob, _carol) = setup();
        emit(&mut app, alice, "grin", Some("self"));
        assert_eq!(by_target(&app, alice), vec!["You grin to yourself.\n"]);
        assert_eq!(by_target(&app, bob), vec!["Alice grins to themselves.\n"]);
    }

    #[test]
    fn own_name_resolves_to_self_case() {
        let (mut app, alice, _bob, _carol) = setup();
        emit(&mut app, alice, "grin", Some("alice"));
        assert_eq!(by_target(&app, alice), vec!["You grin to yourself.\n"]);
    }

    #[test]
    fn target_prefix_matches_case_insensitively() {
        let (mut app, alice, bob, _carol) = setup();
        emit(&mut app, alice, "grin", Some("BO"));
        assert_eq!(by_target(&app, bob), vec!["Alice grins at you.\n"]);
    }

    #[test]
    fn unknown_target_messages_actor_only() {
        let (mut app, alice, bob, carol) = setup();
        emit(&mut app, alice, "grin", Some("xyzzy"));
        assert_eq!(
            by_target(&app, alice),
            vec!["You don't see anyone by that name here.\n"]
        );
        assert!(by_target(&app, bob).is_empty());
        assert!(by_target(&app, carol).is_empty());
    }

    #[test]
    fn unknown_social_answers_unknown_command() {
        let (mut app, alice, _bob, _carol) = setup();
        emit(&mut app, alice, "dinner", None);
        assert_eq!(
            by_target(&app, alice),
            vec![grim_text::tr("error.unknown_command", &[])]
        );
    }

    #[test]
    fn actor_names_cannot_inject_colour() {
        let (mut app, alice, bob, _carol) = setup();
        app.world_mut()
            .entity_mut(alice)
            .insert(Name("{REve".into()));
        emit(&mut app, alice, "grin", None);
        // `{` doubles on escape, so the markup arrives inert.
        assert_eq!(by_target(&app, bob), vec!["{{REve grins.\n"]);
    }

    #[test]
    fn performed_fact_fires_per_case() {
        let (mut app, alice, bob, _carol) = setup();
        emit(&mut app, alice, "grin", None);
        emit(&mut app, alice, "grin", Some("self"));
        emit(&mut app, alice, "grin", Some("bob"));
        let m = app.world().resource::<Messages<SocialPerformed>>();
        let mut c = m.get_cursor();
        let facts: Vec<(String, Option<Entity>)> =
            c.read(m).map(|f| (f.name.clone(), f.target)).collect();
        assert_eq!(
            facts,
            vec![
                ("grin".into(), None),
                ("grin".into(), Some(alice)),
                ("grin".into(), Some(bob)),
            ]
        );
    }

    #[test]
    fn closed_over_factory_parses_solo_and_targeted() {
        // The plugin's Startup registration closes over each social name;
        // prove the closed-over factory parses the same way here.
        let mut reg = CommandRegistry::<Cmd>::new();
        for name in ["grin", "smile"] {
            let owned = name.to_string();
            reg.register(name, move |rest| {
                Some(Cmd::Social {
                    name: owned.clone(),
                    target: rest.split_whitespace().next().map(str::to_string),
                })
            });
            reg.deprioritize(name);
        }
        assert_eq!(
            reg.resolve("grin", ""),
            Some(Cmd::Social {
                name: "grin".into(),
                target: None
            })
        );
        assert_eq!(
            reg.resolve("grin", "bob loudly"),
            Some(Cmd::Social {
                name: "grin".into(),
                target: Some("bob".into())
            })
        );
    }
}
