//! Channel configuration for player-speech channels.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The scope of a channel - how far the message travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    /// Message travels to the current room.
    Room,
    /// Message travels to all rooms in the current area.
    Area,
    /// Message travels to all connected players.
    Global,
}

impl Scope {
    /// Returns a displayable name for the scope.
    pub fn name(&self) -> &'static str {
        match self {
            Scope::Room => "room",
            Scope::Area => "area",
            Scope::Global => "global",
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// How the actor's name is identified in channel messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Identify {
    /// Use the character's name (in-world, perceived sound).
    Perceived,
    /// Use the player's name (out-of-character, speaking as player).
    Always,
}

impl Identify {
    /// Returns a displayable name for the identify mode.
    pub fn name(&self) -> &'static str {
        match self {
            Identify::Perceived => "perceived",
            Identify::Always => "always",
        }
    }
}

impl fmt::Display for Identify {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Who may speak on a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpeakEligibility {
    /// Everyone may speak.
    All,
    /// Only admins may speak.
    AdminOnly,
    /// Only authenticated players may speak.
    Authenticated,
}

impl SpeakEligibility {
    /// Returns a displayable name for the eligibility.
    pub fn name(&self) -> &'static str {
        match self {
            SpeakEligibility::All => "all",
            SpeakEligibility::AdminOnly => "admin_only",
            SpeakEligibility::Authenticated => "authenticated",
        }
    }
}

impl fmt::Display for SpeakEligibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Who may receive messages on a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ListenEligibility {
    /// Everyone may listen.
    All,
    /// Only admins may listen.
    AdminOnly,
    /// Only authenticated players may listen.
    Authenticated,
    /// Only players in the same room may listen.
    InRoom,
    /// Only players in the same area may listen.
    InArea,
}

impl ListenEligibility {
    /// Returns a displayable name for the eligibility.
    pub fn name(&self) -> &'static str {
        match self {
            ListenEligibility::All => "all",
            ListenEligibility::AdminOnly => "admin_only",
            ListenEligibility::Authenticated => "authenticated",
            ListenEligibility::InRoom => "in_room",
            ListenEligibility::InArea => "in_area",
        }
    }
}

impl fmt::Display for ListenEligibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Configuration for a player-speech channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Channel {
    /// The channel name (e.g., "say", "yell", "ooc", "gossip").
    pub name: String,
    /// The scope of the channel (room, area, or global).
    pub scope: Scope,
    /// How the actor's name is identified (perceived vs always OOC).
    pub identify: Identify,
    /// Whether the channel can be toggled on/off per player.
    pub toggleable: bool,
    /// Who may speak on this channel.
    pub speak: SpeakEligibility,
    /// Who may receive messages on this channel.
    pub listen: ListenEligibility,
    /// The catalog key prefix for formatting (e.g., "channel.say").
    pub key: String,
}

impl Channel {
    /// Create a new channel configuration.
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            scope: Scope::Room,
            identify: Identify::Perceived,
            toggleable: false,
            speak: SpeakEligibility::All,
            listen: ListenEligibility::All,
            key: format!("channel.{}", name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_display() {
        assert_eq!(format!("{}", Scope::Room), "room");
        assert_eq!(format!("{}", Scope::Area), "area");
        assert_eq!(format!("{}", Scope::Global), "global");
    }

    #[test]
    fn identify_display() {
        assert_eq!(format!("{}", Identify::Perceived), "perceived");
        assert_eq!(format!("{}", Identify::Always), "always");
    }

    #[test]
    fn speak_eligibility_display() {
        assert_eq!(format!("{}", SpeakEligibility::All), "all");
        assert_eq!(format!("{}", SpeakEligibility::AdminOnly), "admin_only");
        assert_eq!(
            format!("{}", SpeakEligibility::Authenticated),
            "authenticated"
        );
    }

    #[test]
    fn listen_eligibility_display() {
        assert_eq!(format!("{}", ListenEligibility::All), "all");
        assert_eq!(format!("{}", ListenEligibility::AdminOnly), "admin_only");
        assert_eq!(
            format!("{}", ListenEligibility::Authenticated),
            "authenticated"
        );
        assert_eq!(format!("{}", ListenEligibility::InRoom), "in_room");
        assert_eq!(format!("{}", ListenEligibility::InArea), "in_area");
    }

    #[test]
    fn channel_defaults() {
        let channel = Channel::new("test");
        assert_eq!(channel.name, "test");
        assert_eq!(channel.scope, Scope::Room);
        assert_eq!(channel.identify, Identify::Perceived);
        assert!(!channel.toggleable);
        assert_eq!(channel.speak, SpeakEligibility::All);
        assert_eq!(channel.listen, ListenEligibility::All);
        assert_eq!(channel.key, "channel.test");
    }

    #[test]
    fn channel_customization() {
        let channel = Channel {
            name: "gossip".to_string(),
            scope: Scope::Global,
            identify: Identify::Always,
            toggleable: true,
            speak: SpeakEligibility::Authenticated,
            listen: ListenEligibility::Authenticated,
            key: "channel.gossip".to_string(),
        };
        assert_eq!(channel.name, "gossip");
        assert_eq!(channel.scope, Scope::Global);
        assert_eq!(channel.identify, Identify::Always);
        assert!(channel.toggleable);
        assert_eq!(channel.speak, SpeakEligibility::Authenticated);
        assert_eq!(channel.listen, ListenEligibility::Authenticated);
    }
}
