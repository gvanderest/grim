//! Per-recipient render of combat facts: strikes/kicks ([`Damaged`]),
//! deaths ([`Died`]), fight starts ([`FightStart`]), flees ([`Fled`]).
//!
//! A separate system from [`format_output`](crate::output::format_output)
//! because that one is already at Bevy's system-parameter ceiling. The actor
//! sees first-party ("You hit …"), the victim second-party ("… hits you …"),
//! and the rest of the room the attributed line.

use bevy::prelude::*;
use grim_actor::Character;
use grim_combat::{Damaged, Died, FightStart, Fled};
use grim_networking::ConnectionOutput;
use grim_text::tr;

use crate::output::{broadcast_to_room, find_conn, Occupants};

/// Fight starts + strikes/kicks: per-recipient render of [`FightStart`] and
/// [`Damaged`] facts. Split from the aftermath half (file cap): this system
/// owns blows, the next owns deaths + flees.
pub(crate) fn format_combat_strikes(
    mut damaged: MessageReader<Damaged>,
    mut fights: MessageReader<FightStart>,
    room_occupants: Occupants,
    characters: Query<&Character>,
    mut outputs: MessageWriter<ConnectionOutput>,
) {
    for ev in fights.read() {
        let conn = find_conn(ev.being, &room_occupants);
        outputs.write(ConnectionOutput {
            prepend_newline: true,
            ..ConnectionOutput::new(
                conn,
                tr!("combat.fight_start.actor", target = ev.target_name.as_str()),
            )
        });
        if characters.get(ev.target).is_ok() {
            let target_conn = find_conn(ev.target, &room_occupants);
            outputs.write(ConnectionOutput {
                prepend_newline: true,
                ..ConnectionOutput::new(
                    target_conn,
                    tr!("combat.fight_start.target", name = ev.name.as_str()),
                )
            });
        }
        let third = tr!(
            "combat.fight_start.room",
            name = ev.name.as_str(),
            target = ev.target_name.as_str()
        );
        broadcast_to_room(
            ev.room,
            &[ev.being, ev.target],
            &third,
            &room_occupants,
            &mut outputs,
        );
    }
    for ev in damaged.read() {
        // Hits and kicks share one template — a kick is just a blow whose
        // verb is `kick` (carried on the event, not the key).
        let (first_key, target_key, third_key) = if ev.hit {
            ("combat.hit.actor", "combat.hit.target", "combat.hit.room")
        } else {
            (
                "combat.miss.actor",
                "combat.miss.target",
                "combat.miss.room",
            )
        };
        let total = ev.amount.to_string();
        // Actor sees their blow by verb + victim name; the victim sees the
        // attacker by name; the room sees both names.
        let conn = find_conn(ev.attacker, &room_occupants);
        outputs.write(ConnectionOutput {
            prepend_newline: true,
            ..ConnectionOutput::new(
                conn,
                tr!(
                    first_key,
                    verb = ev.damage_noun.as_str(),
                    target = ev.victim_name.as_str(),
                    total = total.as_str()
                ),
            )
        });
        if characters.get(ev.victim).is_ok() {
            let victim_conn = find_conn(ev.victim, &room_occupants);
            outputs.write(ConnectionOutput {
                prepend_newline: true,
                ..ConnectionOutput::new(
                    victim_conn,
                    tr!(
                        target_key,
                        verb = ev.damage_noun.as_str(),
                        total = total.as_str(),
                        name = ev.attacker_name.as_str()
                    ),
                )
            });
        }
        let third = tr!(
            third_key,
            verb = ev.damage_noun.as_str(),
            target = ev.victim_name.as_str(),
            total = total.as_str(),
            name = ev.attacker_name.as_str()
        );
        broadcast_to_room(
            room_of(ev.attacker, ev.victim, &room_occupants),
            &[ev.attacker, ev.victim],
            &third,
            &room_occupants,
            &mut outputs,
        );
    }
}

/// Deaths + flees: per-recipient render of [`Died`] and [`Fled`] facts.
/// Split from the strikes half (file cap).
pub(crate) fn format_combat_aftermath(
    mut died: MessageReader<Died>,
    mut fled: MessageReader<Fled>,
    room_occupants: Occupants,
    characters: Query<&Character>,
    mut outputs: MessageWriter<ConnectionOutput>,
) {
    for ev in died.read() {
        // Killer (when a PC): "You have slain …".
        if let Some(killer) = ev.killer {
            if characters.get(killer).is_ok() {
                let conn = find_conn(killer, &room_occupants);
                outputs.write(ConnectionOutput {
                    prepend_newline: true,
                    ..ConnectionOutput::new(
                        conn,
                        tr!("combat.died.actor", victim = ev.victim_name.as_str()),
                    )
                });
            }
        }
        // Victim (when a PC still around to read): "You have been slain …".
        if ev.victim_pc && characters.get(ev.victim).is_ok() {
            let killer_name = ev.killer_name.as_deref().unwrap_or("something");
            let conn = find_conn(ev.victim, &room_occupants);
            outputs.write(ConnectionOutput {
                prepend_newline: true,
                ..ConnectionOutput::new(conn, tr!("combat.died.victim", name = killer_name))
            });
        }
        // Room: "… has been slain by …". Exclude killer + victim (they got
        // their own lines above).
        let killer_name = ev.killer_name.as_deref().unwrap_or("something");
        let third = tr!(
            "combat.died.room",
            victim = ev.victim_name.as_str(),
            name = killer_name
        );
        let mut exclude = vec![ev.victim];
        if let Some(killer) = ev.killer {
            exclude.push(killer);
        }
        broadcast_to_room(ev.room, &exclude, &third, &room_occupants, &mut outputs);
    }
    for ev in fled.read() {
        if ev.success {
            let heading = ev
                .direction
                .map(|d| d.to_string())
                .unwrap_or_else(|| "away".to_string());
            let conn = find_conn(ev.being, &room_occupants);
            outputs.write(ConnectionOutput {
                prepend_newline: true,
                ..ConnectionOutput::new(conn, tr!("combat.flee.done", heading = heading.as_str()))
            });
            let third = tr!(
                "combat.flee.room",
                name = ev.name.as_str(),
                heading = heading.as_str()
            );
            broadcast_to_room(ev.room, &[ev.being], &third, &room_occupants, &mut outputs);
        } else {
            let conn = find_conn(ev.being, &room_occupants);
            outputs.write(ConnectionOutput {
                prepend_newline: true,
                ..ConnectionOutput::new(conn, tr!("combat.flee.failed"))
            });
        }
    }
}

/// The room a blow was exchanged in: the attacker's room, else the victim's.
/// (Flee-success moves the actor first, so the fact rides on the old room.)
fn room_of(attacker: Entity, victim: Entity, occupants: &Occupants) -> Entity {
    occupants
        .get(attacker)
        .map(|(_, ir, ..)| ir.room)
        .or_else(|_| occupants.get(victim).map(|(_, ir, ..)| ir.room))
        .unwrap_or(attacker)
}
