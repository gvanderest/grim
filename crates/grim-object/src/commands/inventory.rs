//! `inventory`: list the short names of what the actor carries.
//!
//! Reads like any other engine verb: the session layer queues the command and
//! this handler answers with an [`InfoMessage`] — a header plus one row per
//! carried object (sorted by short name, so the listing is deterministic), or
//! the empty line when carrying nothing.

use bevy::prelude::*;
use grim_core::components::Name as GrimName;
use grim_core::events::{Command, EngineCommand, InfoMessage};
use grim_text::tr;

use crate::object::{CarriedBy, Object};

/// `inventory`: list carried objects' short names.
pub(crate) fn handle_inventory(
    mut engine: MessageReader<EngineCommand>,
    carried: Query<(&GrimName, &CarriedBy), With<Object>>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Inventory = cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let mut shorts: Vec<&str> = carried
            .iter()
            .filter(|(_, held)| held.carrier == actor)
            .map(|(nm, _)| nm.0.as_str())
            .collect();
        shorts.sort();
        let text = if shorts.is_empty() {
            tr!("inventory.empty")
        } else {
            let mut out = tr!("inventory.list.header");
            for short in shorts {
                out.push_str(&tr!("inventory.list.row", short = short));
            }
            out
        };
        info.write(InfoMessage {
            target: actor,
            text,
        });
    }
}

/// Wire the `inventory` handler and the messages it reads/emits.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_systems(Update, handle_inventory);
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

    fn infos(app: &App) -> Vec<(Entity, String)> {
        let messages = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = messages.get_cursor();
        cursor
            .read(messages)
            .map(|m| (m.target, m.text.clone()))
            .collect()
    }

    #[test]
    fn empty_inventory_reports_nothing_carried() {
        let mut app = test_app();
        let actor = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Inventory,
        });
        app.update();
        assert_eq!(
            infos(&app),
            vec![(actor, "You are carrying nothing.\n".to_string())]
        );
    }

    #[test]
    fn inventory_lists_carried_shorts_sorted() {
        let mut app = test_app();
        let actor = app.world_mut().spawn_empty().id();
        for name in ["sword", "brass lantern", "coin"] {
            app.world_mut()
                .spawn((Object, GrimName(name.into()), CarriedBy { carrier: actor }));
        }
        // Another carrier's object must not leak in.
        let other = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            Object,
            GrimName("shield".into()),
            CarriedBy { carrier: other },
        ));
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Inventory,
        });
        app.update();
        assert_eq!(
            infos(&app),
            vec![(
                actor,
                "You are carrying:\n  brass lantern\n  coin\n  sword\n".to_string()
            )]
        );
    }

    #[test]
    fn inventory_ignores_ground_objects() {
        let mut app = test_app();
        let actor = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((Object, GrimName("rock".into())));
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Inventory,
        });
        app.update();
        assert_eq!(
            infos(&app),
            vec![(actor, "You are carrying nothing.\n".to_string())]
        );
    }
}
