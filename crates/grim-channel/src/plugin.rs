//! `ChannelPlugin`: wires the player-speech command verbs. Each command owns
//! its own systems and message registration via a `register` fn; this plugin
//! just calls them in turn.

use bevy::prelude::*;

use crate::commands;

/// Handles `say`/`yell`/`ooc`/`tell`/`reply`/`gecho` commands, emitting the
/// corresponding channel events plus `InfoMessage` echoes.
pub struct ChannelPlugin;

impl Plugin for ChannelPlugin {
    fn build(&self, app: &mut App) {
        commands::say::register(app);
        commands::yell::register(app);
        commands::ooc::register(app);
        commands::gecho::register(app);
        commands::tell::register(app);
        commands::reply::register(app);
    }
}
