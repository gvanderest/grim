//! Unified channel handler that uses ChannelRegistry for all channels.
//!
//! This replaces the distinct say/yell/ooc handlers with a single system
//! that dispatches based on the channel name from the EngineCommand.

use bevy::prelude::*;

use crate::{ChannelMessage, ChannelRegistry};
use grim_actor::InRoom;
use grim_core::channel::{Channel, Identify, SpeakEligibility};
use grim_core::events::{Command, EngineCommand, InfoMessage};
use grim_text::tr;
use grim_world::Room;

/// Handle all channel commands based on the channel registry.
pub fn handle_channel(
    mut engine: MessageReader<EngineCommand>,
    mut channel_message: MessageWriter<ChannelMessage>,
    mut info: MessageWriter<InfoMessage>,
    channel_registry: Res<ChannelRegistry>,
    inroom: Query<&InRoom>,
    _rooms: Query<&Room>,
) {
    for cmd in engine.read() {
        // Extract channel name and text from the command
        let (channel_name, text) = match &cmd.command {
            Command::Say { text } => ("say", text),
            Command::Yell { text } => ("yell", text),
            Command::Ooc { text } => ("ooc", text),
            _ => continue,
        };

        // Look up the channel configuration
        let channel = match channel_registry.get(channel_name) {
            Some(c) => c.clone(),
            None => {
                // Unknown channel - should not happen in normal operation
                continue;
            }
        };

        // Check speak eligibility
        if !check_speak_eligibility(&channel.speak, cmd.client, &inroom) {
            continue;
        }

        // Emit the channel message
        let channel_for_message = channel.clone();
        channel_message.write(ChannelMessage {
            channel: channel_for_message,
            actor: cmd.client,
            text: text.clone(),
        });

        // Send echo to actor (with proper formatting based on channel)
        let echo_text = format_echo(&channel, text);
        info.write(InfoMessage {
            target: cmd.client,
            text: echo_text,
        });
    }
}

/// Check if the actor can speak on this channel.
fn check_speak_eligibility(
    eligibility: &SpeakEligibility,
    _actor: Entity,
    _inroom: &Query<&InRoom>,
) -> bool {
    match eligibility {
        SpeakEligibility::All => true,
        SpeakEligibility::AdminOnly => {
            // Check if actor has admin role
            // TODO: Implement admin check via Character component
            false
        }
        SpeakEligibility::Authenticated => {
            // Check if actor has a Character (is logged in)
            // TODO: Implement auth check
            false
        }
    }
}

/// Format the echo message for the actor based on channel configuration.
fn format_echo(channel: &Channel, text: &str) -> String {
    match channel.identify {
        Identify::Perceived => {
            // Use the catalog key for perceived (in-world) speech
            tr!("channel.say.first_party", text = text)
        }
        Identify::Always => {
            // OOC always shows "[OOC]" prefix
            format!("[OOC] You: {}\n", text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_channel_emits_channel_message() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<EngineCommand>()
            .add_message::<InfoMessage>()
            .add_message::<ChannelMessage>();
        app.init_resource::<ChannelRegistry>();

        // Add default channel for say command
        app.world_mut()
            .resource_mut::<ChannelRegistry>()
            .add_channel(Channel::new("say"));

        // Add system under test
        app.add_systems(Update, handle_channel);

        // Spawn a test actor
        let actor = app.world_mut().spawn(()).id();

        // Send a command
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Say {
                text: "hello".into(),
            },
        });

        app.update();

        // Check that ChannelMessage was emitted
        let messages = app.world().resource::<Messages<ChannelMessage>>();
        let mut cursor = messages.get_cursor();
        let iter = cursor.read(messages);
        let msgs: Vec<_> = iter.collect();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].actor, actor);
        assert_eq!(msgs[0].text, "hello");
    }
}
