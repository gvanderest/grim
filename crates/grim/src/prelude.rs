//! The engine's prelude: the `grim-core` prelude plus the domain types
//! Placement Phase 1 relocated into the subsystem crates, so downstream authors
//! keep a single `use grim::prelude::*;` surface.

pub use grim_actor::{
    Actor, Character, CombatSlow, Creature, Engaged, Health, InRoom, Linkdead, OutputHistory,
    Player, Posture, Role, StoredCharacter, StoredObject,
};
pub use grim_auth::ReservedNamePrefixes;
pub use grim_channel::LastWhisperFrom;
pub use grim_combat::{Combat, CombatPlugin, Damaged, Died, FightStart, Fled};
pub use grim_command_events::*;
pub use grim_core::prelude::*;
pub use grim_object::{CarriedBy, Container, Object, OneWay};
pub use grim_scene::ConnectedAt;
pub use grim_script::{compile, CompiledTrigger, ScriptTriggers, TriggerDef, TriggerKind};
pub use grim_world::{
    Area, ClassDef, ClassRegistry, Door, Doors, Exits, RaceDef, RaceRegistry, Room, RoomLocation,
    StartingRoom,
};
