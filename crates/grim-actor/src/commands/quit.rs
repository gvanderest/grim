//! The `quit` command: request a clean disconnect of the actor's underlying
//! connection. Reads the actor's [`Player`] to find the connection to close.

use bevy::prelude::*;
use grim_engine_types::events::{Command, EngineCommand};
use grim_networking::DisconnectRequest;

use crate::player::Player;

/// `quit`: request a clean disconnect of the actor's underlying connection.
pub(crate) fn handle_quit(
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

/// Wire the `quit` handler and the messages it reads/emits.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<DisconnectRequest>()
        .add_systems(Update, handle_quit);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        register(&mut app);
        app
    }

    fn disconnect_targets(app: &App) -> Vec<Entity> {
        let messages = app.world().resource::<Messages<DisconnectRequest>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|m| m.connection).collect()
    }

    #[test]
    fn quit_requests_disconnect_of_the_players_connection() {
        let mut app = test_app();
        let conn = app.world_mut().spawn(()).id();
        let actor = app
            .world_mut()
            .spawn(Player {
                connection: Some(conn),
            })
            .id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Quit,
        });
        app.update();
        assert_eq!(disconnect_targets(&app), vec![conn]);
    }

    #[test]
    fn quit_for_linkdead_player_emits_nothing() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(Player { connection: None }).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Quit,
        });
        app.update();
        assert!(disconnect_targets(&app).is_empty());
    }

    #[test]
    fn quit_for_non_player_emits_nothing() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(()).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Quit,
        });
        app.update();
        assert!(disconnect_targets(&app).is_empty());
    }
}
