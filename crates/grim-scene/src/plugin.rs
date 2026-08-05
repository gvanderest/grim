//! `ScenePlugin`: wires up the session-lifecycle resources, message types, and
//! systems that make up the scene subsystem.

use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::prelude::*;
use grim_engine_types::components::ReservedNamePrefixes;
use grim_engine_types::events::{
    EngineCommand, InfoMessage, LinkdeadAnnounce, LookEntity, LookRoom, MoveEvent, OocEvent,
    SayEvent, ServerBroadcast, YellEvent,
};
use grim_engine_types::events::{LoginAnnounce, LogoutAnnounce};
use grim_networking::{ConnectionOutput, ConnectionResumed, DisconnectRequest};

use crate::command::process_command_queue;
use crate::input::{handle_client_input, handle_connection_established};
use crate::output::{capture_output, format_output, format_server_broadcast};
use crate::parser;
use crate::resume::handle_connection_resumed;

pub struct ScenePlugin;
impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(parser::command_registry());
        // Scene writes account/character JSON, so it needs the persistence
        // directory. init_resource so ScenePlugin stands alone; if
        // PersistencePlugin is also present its identical default is a no-op.
        app.init_resource::<grim_persistence::PersistenceConfig>();
        // Reserved character-name prefixes. init_resource keeps the built-in
        // defaults unless the author inserted a custom list before this plugin.
        app.init_resource::<ReservedNamePrefixes>();
        app.add_message::<ConnectionOutput>()
            .add_message::<ConnectionResumed>()
            .add_message::<DisconnectRequest>()
            .add_message::<EngineCommand>()
            .add_message::<LookRoom>()
            .add_message::<LookEntity>()
            .add_message::<SayEvent>()
            .add_message::<YellEvent>()
            .add_message::<OocEvent>()
            .add_message::<MoveEvent>()
            .add_message::<InfoMessage>()
            .add_message::<LoginAnnounce>()
            .add_message::<LogoutAnnounce>()
            .add_message::<LinkdeadAnnounce>()
            .add_message::<ServerBroadcast>()
            .add_systems(
                Update,
                (
                    handle_connection_established,
                    handle_connection_resumed,
                    handle_client_input.after(handle_connection_established),
                    process_command_queue,
                    format_output,
                    format_server_broadcast,
                    capture_output,
                ),
            );
    }
}
