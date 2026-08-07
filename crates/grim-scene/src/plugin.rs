//! `ScenePlugin`: wires up the session-lifecycle resources, message types, and
//! systems that make up the scene subsystem. The pre-game flow (login /
//! creation / character-select / MOTD) lives in the auth crate; this plugin owns
//! the in-game input dispatch, output formatting, and copyover resume.

use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::prelude::*;
use grim_core::events::{
    EngineCommand, GlobalEcho, InfoMessage, LinkdeadAnnounce, LookEntity, LookRoom, MoveEvent,
    OocEvent, SayEvent, ServerBroadcast, YellEvent,
};
use grim_core::events::{LoginAnnounce, LogoutAnnounce};
use grim_networking::{ConnectionOutput, ConnectionResumed, DisconnectRequest};
use grim_world::{ClassRegistry, RaceRegistry};

use crate::command::process_command_queue;
use crate::input::handle_ingame_input;
use crate::output::{capture_output, format_output, format_server_broadcast};
use crate::parser;
use crate::resume::handle_connection_resumed;
use crate::session::JustEnteredWorld;

/// Public ordering handle for the scene's in-game input dispatch. The auth
/// pre-game input system runs `.before(SceneSystems::InGameInput)` so a line
/// that advances a session into the world is consumed there and not
/// re-dispatched as an in-game command the same tick (see [`JustEnteredWorld`]
/// and `input.rs`).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneSystems {
    /// The in-game input dispatcher ([`handle_ingame_input`]).
    InGameInput,
}

pub struct ScenePlugin;
impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(parser::command_registry());
        // Scene writes character JSON on `quit` and reads it on copyover resume,
        // so it needs the persistence directory. init_resource so ScenePlugin
        // stands alone; if PersistencePlugin is also present its identical
        // default is a no-op.
        app.init_resource::<grim_persistence::PersistenceConfig>();
        // Playable races/classes: the WHO list reads their abbreviations. The
        // character-creation flow (auth crate) also reads them and validates the
        // effective set is non-empty; both plugins init_resource idempotently.
        app.init_resource::<RaceRegistry>();
        app.init_resource::<ClassRegistry>();
        // Per-tick set of connections a pre-game handler advanced to InGame; the
        // in-game input system consults it to avoid re-dispatching the line that
        // triggered the transition (see input.rs).
        app.init_resource::<JustEnteredWorld>();
        app.add_message::<ConnectionOutput>()
            .add_message::<ConnectionResumed>()
            .add_message::<DisconnectRequest>()
            .add_message::<EngineCommand>()
            .add_message::<LookRoom>()
            .add_message::<LookEntity>()
            .add_message::<SayEvent>()
            .add_message::<YellEvent>()
            .add_message::<OocEvent>()
            .add_message::<GlobalEcho>()
            .add_message::<MoveEvent>()
            .add_message::<InfoMessage>()
            .add_message::<LoginAnnounce>()
            .add_message::<LogoutAnnounce>()
            .add_message::<LinkdeadAnnounce>()
            .add_message::<ServerBroadcast>()
            .add_systems(
                Update,
                (
                    handle_connection_resumed,
                    handle_ingame_input.in_set(SceneSystems::InGameInput),
                    process_command_queue,
                    format_output,
                    format_server_broadcast,
                    capture_output,
                ),
            );
    }
}
