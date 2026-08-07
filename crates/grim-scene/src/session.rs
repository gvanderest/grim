//! Session-scoped components + resources stamped by the scene subsystem.

use std::collections::HashSet;

use bevy::prelude::*;
use chrono::{DateTime, Utc};

/// When the character's current session entered the world. Transient (NOT
/// persisted): stamped with `Utc::now()` wherever a character gets its live
/// `Player` — fresh login, takeover, spawn-from-disk, linkdead reconnect, and
/// copyover resume. The WHO list uses it as the connect-time sort tiebreak
/// (oldest connection first among characters of equal level).
#[derive(Component, Debug, Clone, Copy)]
pub struct ConnectedAt(pub DateTime<Utc>);

/// Connections a pre-game handler advanced to [`ClientState::InGame`] this tick.
///
/// The routing split runs the pre-game (auth) input system before the
/// in-game (scene) input system. A single line can advance a session into the
/// world (the MOTD ENTER, or a login-by-name reconnect); the pre-game system
/// consumes that line and records its connection here so the in-game system
/// does not re-dispatch the same line as a command that tick. The pre-game
/// system clears the set at the top of each tick, so it only ever holds this
/// tick's transitions.
///
/// [`ClientState::InGame`]: grim_core::components::ClientState
#[derive(Resource, Default)]
pub struct JustEnteredWorld(pub HashSet<Entity>);
