//! Session-scoped components stamped by the scene subsystem.

use bevy::prelude::*;
use chrono::{DateTime, Utc};

/// When the character's current session entered the world. Transient (NOT
/// persisted): stamped with `Utc::now()` wherever a character gets its live
/// `Player` — fresh login, takeover, spawn-from-disk, linkdead reconnect, and
/// copyover resume. The WHO list uses it as the connect-time sort tiebreak
/// (oldest connection first among characters of equal level).
#[derive(Component, Debug, Clone, Copy)]
pub struct ConnectedAt(pub DateTime<Utc>);
