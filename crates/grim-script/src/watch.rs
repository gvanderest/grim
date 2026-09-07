//! Room-transition watcher: match scripted creatures, run triggers, route output.
//!
//! For each transition event, every [`Creature`] in the affected room — except
//! the mover itself — fires its triggers for that moment in blueprint order.
//! Script speech goes out as a `say` [`ChannelMessage`] from the mob (rendered
//! by the existing channel path, so mobs sound exactly like players). Any
//! failure — a Lua error, or a missing `say` channel — pages every online
//! admin via [`InfoMessage`] and the server log, and never disables anything.

use bevy::prelude::*;
use grim_actor::{AttemptEnter, AttemptLeave, Character, Creature, Enter, InRoom, Leave, Player};
use grim_channel::{ChannelMessage, ChannelRegistry};
use grim_core::components::Name as GrimName;
use grim_core::events::InfoMessage;
use grim_text::tr;

use crate::runtime::run_trigger;
use crate::trigger::{ScriptTriggers, TriggerKind};

/// Watch all four transition moments. One system (not four) so the matching
/// rule lives in exactly one place; the per-moment fan-out below is the only
/// repetition, and it names the room each moment watches.
#[allow(clippy::too_many_arguments)]
pub(crate) fn watch_transitions(
    mut attempt_enter: MessageReader<AttemptEnter>,
    mut attempt_leave: MessageReader<AttemptLeave>,
    mut enter: MessageReader<Enter>,
    mut leave: MessageReader<Leave>,
    creatures: Query<(Entity, &ScriptTriggers, &InRoom), With<Creature>>,
    mob_names: Query<&GrimName>,
    admins: Query<(Entity, &Character, &Player)>,
    registry: Option<Res<ChannelRegistry>>,
    mut speech: MessageWriter<ChannelMessage>,
    mut info: MessageWriter<InfoMessage>,
) {
    for ev in attempt_leave.read() {
        fire(
            TriggerKind::AttemptLeave,
            ev.actor,
            ev.room,
            &creatures,
            &mob_names,
            &admins,
            registry.as_deref(),
            &mut speech,
            &mut info,
        );
    }
    for ev in attempt_enter.read() {
        fire(
            TriggerKind::AttemptEnter,
            ev.actor,
            ev.room,
            &creatures,
            &mob_names,
            &admins,
            registry.as_deref(),
            &mut speech,
            &mut info,
        );
    }
    for ev in leave.read() {
        fire(
            TriggerKind::Leave,
            ev.actor,
            ev.room,
            &creatures,
            &mob_names,
            &admins,
            registry.as_deref(),
            &mut speech,
            &mut info,
        );
    }
    for ev in enter.read() {
        fire(
            TriggerKind::Enter,
            ev.actor,
            ev.room,
            &creatures,
            &mob_names,
            &admins,
            registry.as_deref(),
            &mut speech,
            &mut info,
        );
    }
}

/// Fire every trigger for `on` on each scripted creature standing in `room`,
/// skipping the mover itself (a mob never greets its own steps).
#[allow(clippy::too_many_arguments)]
fn fire(
    on: TriggerKind,
    mover: Entity,
    room: Entity,
    creatures: &Query<(Entity, &ScriptTriggers, &InRoom), With<Creature>>,
    mob_names: &Query<&GrimName>,
    admins: &Query<(Entity, &Character, &Player)>,
    registry: Option<&ChannelRegistry>,
    speech: &mut MessageWriter<ChannelMessage>,
    info: &mut MessageWriter<InfoMessage>,
) {
    let channel = registry.and_then(|r| r.get("say")).cloned();
    for (mob, triggers, inroom) in creatures.iter() {
        if mob == mover || inroom.room != room {
            continue;
        }
        for trigger in &triggers.0 {
            if trigger.on != on {
                continue;
            }
            match run_trigger(&trigger.bytecode, on) {
                Ok(said) => match &channel {
                    Some(say) => {
                        for text in said {
                            speech.write(ChannelMessage {
                                channel: say.clone(),
                                actor: mob,
                                text,
                            });
                        }
                    }
                    None => report(
                        mob,
                        on,
                        "the `say` channel is not registered",
                        mob_names,
                        admins,
                        info,
                    ),
                },
                Err(error) => report(mob, on, &error, mob_names, admins, info),
            }
        }
    }
}

/// A trigger failed: log it for the operator and page every online admin.
/// Nothing is disabled — the next transition fires the script again.
fn report(
    mob: Entity,
    on: TriggerKind,
    error: &str,
    mob_names: &Query<&GrimName>,
    admins: &Query<(Entity, &Character, &Player)>,
    info: &mut MessageWriter<InfoMessage>,
) {
    let name = mob_names_or_entity(mob_names, mob);
    bevy::log::error!("script trigger failed: {name} ({}): {error}", on.as_str());
    let text = tr!(
        "script.trigger.failed",
        name = name,
        trigger = on.as_str(),
        error = error
    );
    for (admin, character, _) in admins.iter() {
        if character.is_admin() {
            info.write(InfoMessage {
                target: admin,
                text: text.clone(),
            });
        }
    }
}

/// The mob's display name, or its entity id when nameless (never empty — the
/// admin page must always identify the culprit).
fn mob_names_or_entity(names: &Query<&GrimName>, mob: Entity) -> String {
    names
        .get(mob)
        .map(|n| n.0.clone())
        .unwrap_or_else(|_| format!("entity:{}", mob.index()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::{compile, CompiledTrigger};

    fn trigger(on: TriggerKind, script: &str) -> CompiledTrigger {
        CompiledTrigger {
            on,
            bytecode: compile(script).expect("fixture must compile"),
        }
    }

    fn scripted_mob(app: &mut App, room: Entity, triggers: Vec<CompiledTrigger>) -> Entity {
        app.world_mut()
            .spawn((
                Creature,
                GrimName("Grimmok".into()),
                InRoom { room },
                ScriptTriggers(triggers),
            ))
            .id()
    }

    fn spawned_character(app: &mut App, roles: Vec<grim_actor::Role>) -> Entity {
        let conn = app.world_mut().spawn_empty().id();
        let actor = app
            .world_mut()
            .spawn((
                Character {
                    id: grim_core::id::GrimId::new(),
                    account_id: grim_core::id::GrimId::new(),
                    created_at: chrono::Utc::now(),
                    last_room: None,
                    roles,
                    class: String::new(),
                    title: None,
                    restrings: Default::default(),
                },
                GrimName("Root".into()),
                Player { connection: conn },
            ))
            .id();
        actor
    }

    fn said(app: &mut App) -> Vec<(Entity, String)> {
        let messages = app.world().resource::<Messages<ChannelMessage>>();
        let mut cursor = messages.get_cursor();
        cursor
            .read(messages)
            .map(|m| (m.actor, m.text.clone()))
            .collect()
    }

    fn infos(app: &mut App) -> Vec<(Entity, String)> {
        let messages = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = messages.get_cursor();
        cursor
            .read(messages)
            .map(|m| (m.target, m.text.clone()))
            .collect()
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AttemptEnter>()
            .add_message::<AttemptLeave>()
            .add_message::<Enter>()
            .add_message::<Leave>()
            .add_message::<ChannelMessage>()
            .add_message::<InfoMessage>()
            .init_resource::<ChannelRegistry>()
            .add_systems(Update, watch_transitions);
        app.world_mut()
            .resource_mut::<ChannelRegistry>()
            .add_channel(grim_channel::Channel {
                name: "say".to_string(),
                scope: grim_core::channel::Scope::Room,
                identify: grim_core::channel::Identify::Perceived,
                toggleable: false,
                speak: grim_core::channel::SpeakEligibility::All,
                listen: grim_core::channel::ListenEligibility::All,
                key: "channel.say".to_string(),
            });
        app
    }

    #[test]
    fn enter_fires_matching_trigger_in_arrival_room() {
        let mut app = test_app();
        let dest = app.world_mut().spawn_empty().id();
        let mob = scripted_mob(
            &mut app,
            dest,
            vec![trigger(TriggerKind::Enter, "say('hi')")],
        );
        let mover = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(Enter {
            actor: mover,
            room: dest,
        });
        app.update();
        assert_eq!(said(&mut app), [(mob, "hi".to_string())]);
    }

    #[test]
    fn wrong_room_and_wrong_moment_stay_silent() {
        let mut app = test_app();
        let here = app.world_mut().spawn_empty().id();
        let elsewhere = app.world_mut().spawn_empty().id();
        scripted_mob(
            &mut app,
            elsewhere,
            vec![trigger(TriggerKind::Enter, "say('hi')")],
        );
        scripted_mob(
            &mut app,
            here,
            vec![trigger(TriggerKind::Leave, "say('bye')")],
        );
        let mover = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(Enter {
            actor: mover,
            room: here,
        });
        app.update();
        assert!(said(&mut app).is_empty());
    }

    #[test]
    fn mover_never_fires_its_own_triggers() {
        let mut app = test_app();
        let room = app.world_mut().spawn_empty().id();
        let mover = app
            .world_mut()
            .spawn((
                Creature,
                GrimName("Restless".into()),
                InRoom { room },
                ScriptTriggers(vec![trigger(TriggerKind::Enter, "say('me')")]),
            ))
            .id();
        app.world_mut().write_message(Enter { actor: mover, room });
        app.update();
        assert!(said(&mut app).is_empty());
    }

    #[test]
    fn attempt_leave_fires_in_departure_room() {
        let mut app = test_app();
        let src = app.world_mut().spawn_empty().id();
        let mob = scripted_mob(
            &mut app,
            src,
            vec![trigger(TriggerKind::AttemptLeave, "say('bye')")],
        );
        let mover = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(AttemptLeave {
            actor: mover,
            room: src,
        });
        app.update();
        assert_eq!(said(&mut app), [(mob, "bye".to_string())]);
    }

    #[test]
    fn failing_script_pages_online_admins() {
        let mut app = test_app();
        let room = app.world_mut().spawn_empty().id();
        scripted_mob(&mut app, room, vec![trigger(TriggerKind::Enter, "nope()")]);
        let admin = spawned_character(&mut app, vec![grim_actor::Role::Admin]);
        let mover = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(Enter { actor: mover, room });
        app.update();
        let pages = infos(&mut app);
        assert_eq!(pages.len(), 1, "exactly one admin page");
        assert_eq!(pages[0].0, admin, "paged to the admin");
        assert!(
            pages[0].1.contains("Grimmok"),
            "names the mob: {}",
            pages[0].1
        );
        assert!(
            pages[0].1.contains("enter"),
            "names the moment: {}",
            pages[0].1
        );
    }

    #[test]
    fn linkdead_admin_and_players_get_no_page() {
        let mut app = test_app();
        let room = app.world_mut().spawn_empty().id();
        scripted_mob(&mut app, room, vec![trigger(TriggerKind::Enter, "nope()")]);
        // Admin without a `Player` (linkdead): the page targets live sessions.
        app.world_mut().spawn((
            GrimName("Gone".into()),
            Character {
                id: grim_core::id::GrimId::new(),
                account_id: grim_core::id::GrimId::new(),
                created_at: chrono::Utc::now(),
                last_room: None,
                roles: vec![grim_actor::Role::Admin],
                class: String::new(),
                title: None,
                restrings: Default::default(),
            },
        ));
        spawned_character(&mut app, Vec::new());
        let mover = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(Enter { actor: mover, room });
        app.update();
        assert!(infos(&mut app).is_empty());
    }
}
