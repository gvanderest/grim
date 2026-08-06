//! System-parameter bundles.
//!
//! Bevy caps a system at 16 parameters. Several scene systems need more
//! resources/queries than that, so cohesive groups are bundled into
//! `#[derive(SystemParam)]` structs here and shared across the dispatcher,
//! resume, and output systems.

use bevy::prelude::*;
use grim_actor::{Actor, Character, InRoom, Linkdead, OutputHistory, Player};
use grim_core::components::Name as GrimName;
use grim_core::events::{Command, LinkdeadAnnounce, LoginAnnounce, LogoutAnnounce};
use grim_networking::DisconnectRequest;
use grim_world::{Area, ClassRegistry, RaceRegistry, Room, RoomLocation, StartingRoom};

use crate::session::ConnectedAt;
use crate::validation::ReservedNamePrefixes;

/// The online-characters query shared by the WHO / WHERE renderers and the
/// post-login auto-look: each player entity with its display name, current
/// room, optional [`Actor`] base (race/level/gender WHO stats), optional
/// [`Character`] (admin/title/restrings WHO stats), and optional [`ConnectedAt`]
/// (the WHO connect-time sort tiebreak).
pub(crate) type PlayerChars<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GrimName,
        &'static InRoom,
        Option<&'static Actor>,
        Option<&'static Character>,
        Option<&'static ConnectedAt>,
    ),
>;

/// Session-scoped resources bundled into one `SystemParam` so the input
/// dispatcher can take the command registry as a real `Res` without exceeding
/// Bevy's 16-parameter system limit.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct SessionRes<'w> {
    pub(crate) starting: Res<'w, StartingRoom>,
    pub(crate) registry: Res<'w, grim_command::CommandRegistry<Command>>,
    pub(crate) persistence: Res<'w, grim_persistence::PersistenceConfig>,
    pub(crate) reserved: Res<'w, ReservedNamePrefixes>,
    pub(crate) races: Res<'w, RaceRegistry>,
    pub(crate) classes: Res<'w, ClassRegistry>,
}

/// Rooms + areas bundled so placement code can resolve a persisted
/// [`RoomLocation`] (stable area/room `friendly_id`s) to the *current* room
/// entity. Bundled as one `SystemParam` to stay within Bevy's 16-parameter
/// system limit, and it also serves the plain room lookups the dispatcher does.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct RoomResolver<'w, 's> {
    pub(crate) rooms: Query<'w, 's, (Entity, &'static Room, &'static GrimName)>,
    pub(crate) areas: Query<'w, 's, &'static Area>,
}

impl RoomResolver<'_, '_> {
    /// The room entity matching this location's area + room `friendly_id`s, if
    /// it exists in the currently-loaded world.
    pub(crate) fn resolve(&self, loc: &RoomLocation) -> Option<Entity> {
        self.rooms.iter().find_map(|(e, r, _)| {
            let area_matches = self
                .areas
                .get(r.area)
                .map(|a| a.friendly_id == loc.area)
                .unwrap_or(false);
            (r.friendly_id == loc.room && area_matches).then_some(e)
        })
    }

    /// Where to place a character entering the world: their persisted
    /// `last_room` if it still resolves, else the starting room. New characters
    /// (no `last_room`) and characters whose room no longer exists both fall
    /// back to `starting`.
    pub(crate) fn placement(&self, last_room: Option<&RoomLocation>, starting: Entity) -> Entity {
        last_room
            .and_then(|loc| self.resolve(loc))
            .unwrap_or(starting)
    }
}

/// Everything [`crate::world_entry::enter_world_by_name`] needs to place a
/// character in the world (reconnect / takeover / spawn-from-disk), bundled into
/// one `SystemParam`. Shared by the login-by-name, character-select, and
/// legacy-backfill (class-pick) paths so none of them thread seven separate
/// params. Same pattern as [`SessionRes`] / [`RoomResolver`]; also keeps
/// `handle_client_input` within Bevy's 16-parameter limit.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct WorldEntry<'w, 's> {
    pub(crate) characters: Query<
        'w,
        's,
        (
            Entity,
            &'static Character,
            &'static Actor,
            &'static GrimName,
        ),
    >,
    pub(crate) players: Query<'w, 's, &'static Player>,
    pub(crate) linkdead: Query<'w, 's, &'static Linkdead>,
    pub(crate) rooms: RoomResolver<'w, 's>,
    pub(crate) histories: Query<'w, 's, &'static mut OutputHistory>,
    pub(crate) announce_linkdead: MessageWriter<'w, LinkdeadAnnounce>,
    pub(crate) disconnect: MessageWriter<'w, DisconnectRequest>,
}

/// The three global-announce readers bundled into one `SystemParam`, so
/// `format_output` stays within Bevy's 16-parameter system limit.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AnnounceReaders<'w, 's> {
    pub(crate) login: MessageReader<'w, 's, LoginAnnounce>,
    pub(crate) logout: MessageReader<'w, 's, LogoutAnnounce>,
    pub(crate) linkdead: MessageReader<'w, 's, LinkdeadAnnounce>,
}
