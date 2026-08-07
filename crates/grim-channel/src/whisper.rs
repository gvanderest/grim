//! Shared whisper plumbing for `tell`/`reply`: the [`LastWhisperFrom`] marker
//! and the [`deliver_whisper`] delivery routine both commands use.

use bevy::prelude::*;
use grim_actor::{Character, Linkdead, Player};
use grim_core::components::Name;
use grim_core::events::InfoMessage;

/// Query filter for "a player character currently in the world" — a `Character`
/// that is either connected (`Player`) or `Linkdead`. Excludes mobs (which carry
/// `Creature`, not `Character`) and stale `Character`-only entities. Shared by
/// the `tell`/`reply` target lookups.
pub(crate) type LivePc = (With<Character>, Or<(With<Player>, With<Linkdead>)>);

/// The last player who whispered (`tell`/`whisper`) this character, so `reply`
/// can answer them. Set on delivery; points at a (boot-local) player entity, so
/// a reply fails gracefully if they've since left.
#[derive(Component, Debug)]
pub struct LastWhisperFrom(pub Entity);

/// Deliver one whisper: echo `You tell <Name> '<text>'` to the sender, and —
/// for a distinct recipient — `<Sender> tells you '<text>'` plus record the
/// sender as the recipient's [`LastWhisperFrom`] so they can `reply`. A whisper
/// to `self` echoes only the "You tell …" line.
pub(crate) fn deliver_whisper(
    actor: Entity,
    recipient: Entity,
    text: &str,
    names: &Query<&Name>,
    info: &mut MessageWriter<InfoMessage>,
    commands: &mut Commands,
) {
    let recipient_name = names
        .get(recipient)
        .map(|n| n.0.clone())
        .unwrap_or_default();
    info.write(InfoMessage {
        target: actor,
        text: format!("You tell {recipient_name} '{text}'\n"),
    });
    if recipient != actor {
        let sender_name = names.get(actor).map(|n| n.0.clone()).unwrap_or_default();
        info.write(InfoMessage {
            target: recipient,
            text: format!("{sender_name} tells you '{text}'\n"),
        });
        commands.entity(recipient).insert(LastWhisperFrom(actor));
    }
}
