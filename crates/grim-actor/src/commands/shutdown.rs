//! The `shutdown` command: an admin schedules a graceful server shutdown. This
//! is the *being-reading* half of the shutdown subsystem — it gates on the
//! actor's [`Character`] admin role. The signal handling, countdown state, and
//! ticking stay in `grim_world`'s `ShutdownPlugin` (they are being-free).
//!
//! The handler slots into [`grim_world::ShutdownSet::Command`], between the
//! world's SIGTERM poll and countdown tick, so a SIGTERM and an admin
//! `shutdown` arriving in the same tick still schedule exactly one countdown.

use bevy::prelude::*;
use grim_engine_types::events::{Command, EngineCommand, InfoMessage, ServerBroadcast};
use grim_world::shutdown::{warn_text, ActiveShutdown, ShutdownCountdown};
use grim_world::ShutdownSet;

use crate::character::Character;

/// `shutdown <seconds>`: admin-gated (defense in depth — the client gates first).
/// Non-admins are ignored silently; a second request while one is pending is
/// rejected.
fn handle_shutdown_command(
    mut engine: MessageReader<EngineCommand>,
    characters: Query<&Character>,
    active: Option<Res<ActiveShutdown>>,
    mut info: MessageWriter<InfoMessage>,
    mut broadcast: MessageWriter<ServerBroadcast>,
    mut commands: Commands,
) {
    // `active` reflects last tick's resource; `insert_resource` below only lands
    // at the next sync point. Track within this batch too, so two admin
    // `shutdown`s read in the same tick schedule exactly one countdown (the
    // first) rather than stacking two.
    let mut scheduled = false;
    for cmd in engine.read() {
        let Command::Shutdown { seconds } = cmd.command else {
            continue;
        };
        let actor = cmd.client;
        // Defense in depth. The client already gates `shutdown` and masks it as
        // an unknown command for non-admins, so a well-behaved session never
        // sends this for a non-admin. If one arrives anyway (a non-client
        // command source), fail closed and stay silent — emitting anything here
        // would leak the command's existence with the wrong output framing.
        let is_admin = characters
            .get(actor)
            .map(Character::is_admin)
            .unwrap_or(false);
        if !is_admin {
            continue;
        }
        if active.is_some() || scheduled {
            info.write(InfoMessage {
                target: actor,
                text: "A shutdown is already scheduled.\n".into(),
            });
            continue;
        }
        broadcast.write(ServerBroadcast {
            text: warn_text(seconds),
        });
        commands.insert_resource(ActiveShutdown(ShutdownCountdown::new(seconds)));
        scheduled = true;
    }
}

/// Wire the `shutdown` handler into [`ShutdownSet::Command`] and register the
/// messages it reads/emits. `ServerBroadcast` and the countdown resources are
/// owned by `grim_world`'s `ShutdownPlugin`, which must also be present.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_systems(Update, handle_shutdown_command.in_set(ShutdownSet::Command));
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use grim_engine_types::character::Gender;
    use grim_engine_types::GrimId;
    use grim_world::ShutdownPlugin;

    use crate::character::Role;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ShutdownPlugin);
        register(&mut app);
        app
    }

    fn spawn_character(app: &mut App, roles: Vec<Role>) -> Entity {
        app.world_mut()
            .spawn(Character {
                id: GrimId::new(),
                name: "Tester".into(),
                account_id: GrimId::new(),
                created_at: Utc::now(),
                last_room: None,
                roles,
                gender: Gender::Neutral,
                race: String::new(),
                class: String::new(),
                level: 1,
                title: None,
                restrings: std::collections::HashMap::new(),
            })
            .id()
    }

    fn drain<M: Message + std::fmt::Debug>(app: &App) -> Vec<String> {
        let messages = app.world().resource::<Messages<M>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|m| format!("{m:?}")).collect()
    }

    #[test]
    fn non_admin_is_denied_and_nothing_scheduled() {
        let mut app = test_app();
        let actor = spawn_character(&mut app, Vec::new());
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Shutdown { seconds: 30 },
        });
        app.update();

        // Silent fail-closed: no schedule, and no output at all (the client
        // owns the unknown-command masking; the engine must not emit anything
        // that would leak the command's existence).
        assert!(app.world().get_resource::<ActiveShutdown>().is_none());
        assert_eq!(drain::<InfoMessage>(&app).len(), 0);
        assert_eq!(drain::<ServerBroadcast>(&app).len(), 0);
    }

    #[test]
    fn admin_schedules_and_broadcasts() {
        let mut app = test_app();
        let actor = spawn_character(&mut app, vec![Role::Admin]);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Shutdown { seconds: 30 },
        });
        app.update();

        assert!(app.world().get_resource::<ActiveShutdown>().is_some());
        let casts = drain::<ServerBroadcast>(&app);
        assert!(casts.iter().any(|c| c.contains("30")));
    }

    #[test]
    fn second_shutdown_is_rejected() {
        let mut app = test_app();
        let actor = spawn_character(&mut app, vec![Role::Admin]);
        app.world_mut()
            .insert_resource(ActiveShutdown(ShutdownCountdown::new(30)));
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Shutdown { seconds: 10 },
        });
        app.update();

        let infos = drain::<InfoMessage>(&app);
        assert!(infos.iter().any(|i| i.contains("already scheduled")));
    }

    #[test]
    fn two_admin_shutdowns_in_one_tick_schedule_only_one() {
        let mut app = test_app();
        let actor = spawn_character(&mut app, vec![Role::Admin]);
        // Both land in the same batch, read before either `insert_resource`
        // applies. The loop-local guard must still schedule exactly one.
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Shutdown { seconds: 30 },
        });
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Shutdown { seconds: 5 },
        });
        app.update();

        assert!(app.world().get_resource::<ActiveShutdown>().is_some());
        // First wins: one broadcast (for 30s), and the second request is
        // rejected with the "already scheduled" notice.
        let casts = drain::<ServerBroadcast>(&app);
        assert_eq!(casts.len(), 1, "exactly one countdown announced: {casts:?}");
        assert!(casts[0].contains("30"));
        let infos = drain::<InfoMessage>(&app);
        assert!(infos.iter().any(|i| i.contains("already scheduled")));
    }
}
