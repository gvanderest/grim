//! Instant actions: flee, kick, cast, posture.
//!
//! All handlers are world-mut queued closures (they touch engagement,
//! placement, and health atomically). Flee and engaged moves share the 3s
//! `CombatSlow`; kick enforces the class grant + 6s ability cooldown.

use bevy::prelude::*;
use grim_actor::{CombatSlow, Engaged, Health, Posture};
use grim_core::components::Name as GrimName;
use grim_core::events::{Command, EngineCommand, InfoMessage, LookRoom};
use grim_text::tr;
use grim_world::Exits;

use crate::ability::{class_grants, AbilityCooldowns, AbilityRegistry, CombatClock};
use crate::corpse::is_corpse;
use crate::death::kill_being;
use crate::engage::{ensure_engaged, find_victim, strike_once, strip};
use crate::events::{DamageKind, Fled};
use crate::state::CombatRng;

/// 3s reuse delay shared by flee and engaged moves.
pub const FLEE_DELAY: f32 = 3.0;

/// Flee success chance, in percent.
pub const FLEE_CHANCE: u32 = 50;

/// `flee`: combat-only. Random exit, 50% success. `CombatSlow` applies on the
/// attempt either way; success moves + strips engagement.
pub fn handle_flee(
    mut engine: MessageReader<EngineCommand>,
    mut commands: Commands,
    inroom: Query<&grim_actor::InRoom>,
    names: Query<&GrimName>,
    engaged: Query<&Engaged>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Flee = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let (Some(actor_room), Some(actor_name), engaged_now) = (
            inroom.get(actor).ok(),
            names.get(actor).ok(),
            engaged.get(actor).is_ok(),
        ) else {
            continue;
        };
        if !engaged_now {
            info.write(InfoMessage {
                target: actor,
                text: tr!("combat.not_fighting"),
            });
            continue;
        }
        let (room, name) = (actor_room.room, actor_name.0.clone());
        flee_now(&mut commands, actor, room, name);
        let _ = &mut info;
    }
}

/// The flee roll itself (queued closure body, factored for the line budget):
/// slow-check, 3s slow, random exit, 50% success, move + strip on success.
fn flee_now(commands: &mut Commands, actor: Entity, room: Entity, name: String) {
    commands.queue(move |world: &mut World| {
        if world
            .get::<CombatSlow>(actor)
            .is_some_and(|s| s.remaining > 0.0)
        {
            world
                .resource_mut::<Messages<InfoMessage>>()
                .write(InfoMessage {
                    target: actor,
                    text: tr!("combat.delay"),
                });
            return;
        }
        world.entity_mut(actor).insert(CombatSlow {
            remaining: FLEE_DELAY,
        });
        let exits: Vec<grim_core::Cardinal> = world
            .get::<Exits>(room)
            .map(|e| e.exits.keys().copied().collect())
            .unwrap_or_default();
        if exits.is_empty() {
            world.resource_mut::<Messages<Fled>>().write(Fled {
                being: actor,
                name: name.clone(),
                room,
                success: false,
                direction: None,
                dest: None,
            });
            return;
        }
        let mut rng = world.resource_mut::<CombatRng>();
        let pick = exits[(rng.0.below(exits.len() as u64)) as usize];
        let dest = world
            .get::<Exits>(room)
            .and_then(|e| e.exits.get(&pick).copied());
        let success = {
            let mut rng = world.resource_mut::<CombatRng>();
            rng.0.chance(FLEE_CHANCE)
        };
        match (success, dest) {
            (true, Some(to)) => {
                if let Some(mut ir) = world.get_mut::<grim_actor::InRoom>(actor) {
                    ir.room = to;
                }
                // Refresh persisted location for PCs.
                let loc = {
                    let room = world
                        .get::<grim_world::Room>(to)
                        .map(|r| (r.friendly_id.clone(), r.area));
                    room.and_then(|(room_slug, area_e)| {
                        world
                            .get::<grim_world::Area>(area_e)
                            .map(|a| grim_world::RoomLocation {
                                area: a.friendly_id.clone(),
                                room: room_slug,
                            })
                    })
                };
                if let Some(loc) = loc {
                    if let Some(mut ch) = world.get_mut::<grim_actor::Character>(actor) {
                        ch.last_room = Some(loc);
                    }
                }
                strip(world, actor);
                world.resource_mut::<Messages<Fled>>().write(Fled {
                    being: actor,
                    name: name.clone(),
                    room,
                    success: true,
                    direction: Some(pick),
                    dest: Some(to),
                });
                // Walk-like arrival: the destination room description lands
                // right after the flee line, same as a successful walk.
                world.resource_mut::<Messages<LookRoom>>().write(LookRoom {
                    target: actor,
                    room: to,
                });
            }
            _ => flee_fact(world, actor, &name, room, false),
        }
    });
}

/// Emit one [`Fled`] fact. Factored for the line budget (shared by both arms).
fn flee_fact(world: &mut World, actor: Entity, name: &str, room: Entity, success: bool) {
    world.resource_mut::<Messages<Fled>>().write(Fled {
        being: actor,
        name: name.to_string(),
        room,
        success,
        direction: None,
        dest: None,
    });
}

/// `kick [<target>]`: combat-only skill, never initiates. Defaults to the
/// primary; requires the class grant + passes the 6s cooldown; 1.5x strike.
pub fn handle_kick(
    mut engine: MessageReader<EngineCommand>,
    mut commands: Commands,
    inroom: Query<&grim_actor::InRoom>,
    names: Query<&GrimName>,
    engaged: Query<&Engaged>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Kick { target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Some(actor_room) = inroom.get(actor).ok() else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("combat.not_fighting"),
            });
            continue;
        };
        let Some(actor_name) = names.get(actor).ok() else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("combat.not_fighting"),
            });
            continue;
        };
        let Ok(list) = engaged.get(actor) else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("combat.not_fighting"),
            });
            continue;
        };
        let (room, attacker_name) = (actor_room.room, actor_name.0.clone());
        let primary = list.primary();
        let target_text = target.clone();
        kick_now(
            &mut commands,
            actor,
            room,
            attacker_name,
            primary,
            target_text,
        );
        let _ = &mut info;
    }
}

/// The kick itself (queued closure body, factored for the line budget):
/// grant → cooldown → victim → 1.5x strike → death routing.
fn kick_now(
    commands: &mut Commands,
    actor: Entity,
    room: Entity,
    attacker_name: String,
    primary: Option<Entity>,
    target_text: Option<String>,
) {
    commands.queue(move |world: &mut World| {
        // Fire-time liveness: the kicker may have died since dispatch.
        if world.get::<Health>(actor).is_some_and(|h| h.is_dead()) {
            return;
        }
        // Grant: PCs need kick on their class at their level; creatures
        // never kick.
        let granted = match world.get::<grim_actor::Character>(actor) {
            Some(ch) => {
                let level = world
                    .get::<grim_actor::Actor>(actor)
                    .map(|a| a.level)
                    .unwrap_or(1);
                class_grants(
                    world.resource::<grim_world::ClassRegistry>(),
                    &ch.class,
                    "kick",
                    level,
                )
            }
            None => false,
        };
        if !granted {
            deny(world, actor, "combat.no_target");
            return;
        }
        // Cooldown (6s on the combat clock).
        let now = world.resource::<CombatClock>().0;
        let on_cooldown = world
            .resource::<AbilityCooldowns>()
            .0
            .get(&(actor, "kick".to_string()))
            .is_some_and(|last| now - last < 6.0);
        if on_cooldown {
            deny(world, actor, "combat.cooldown");
            return;
        }
        // Resolve the victim: explicit target must be engaged, else primary.
        // A stale primary (dead/despawned since dispatch) falls through to
        // "no target" instead of striking the corpse.
        let victim = match target_text.as_deref() {
            Some(raw) if !raw.trim().is_empty() => find_victim(world, actor, room, raw),
            _ => primary.filter(|p| {
                world.get_entity(*p).is_ok()
                    && !is_corpse(world, *p)
                    && !world.get::<Health>(*p).is_some_and(|h| h.is_dead())
            }),
        };
        let Some(victim) = victim else {
            deny(world, actor, "combat.no_target");
            return;
        };
        if let Some(mut p) = world.get_mut::<Posture>(actor) {
            *p = Posture::Standing;
        }
        world
            .resource_mut::<AbilityCooldowns>()
            .0
            .insert((actor, "kick".to_string()), now);
        let victim_name = world
            .get::<GrimName>(victim)
            .map(|n| n.0.clone())
            .unwrap_or_default();
        ensure_engaged(world, actor, victim, room);
        let died = strike_once(
            world,
            actor,
            &attacker_name,
            victim,
            &victim_name,
            DamageKind::Kick,
            1.5,
        );
        if died {
            kill_being(world, victim, Some(actor), room);
        }
    });
}

/// One-line combat refusal. Factored for the line budget (shared by kick's
/// gates and future abilities).
fn deny(world: &mut World, actor: Entity, key: &str) {
    world
        .resource_mut::<Messages<InfoMessage>>()
        .write(InfoMessage {
            target: actor,
            text: tr!(key),
        });
}

/// `cast <spell> [<target>]`: ability path for spells. No spells are seeded
/// yet, so every cast answers unknown-spell; the grant/target/cooldown flow
/// mirrors kick for the first real spell to reuse.
pub fn handle_cast(
    mut engine: MessageReader<EngineCommand>,
    abilities: Res<AbilityRegistry>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Cast { spell, target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        match abilities.get(spell) {
            Some(def) => {
                // Seeded spells route here once they exist; today only kick
                // (a skill) is registered, so any matched def is unexpected.
                let _ = (def, target);
                info.write(InfoMessage {
                    target: actor,
                    text: tr!("combat.cast.unknown", name = spell.as_str()),
                })
            }
            None => info.write(InfoMessage {
                target: actor,
                text: tr!("combat.cast.unknown", name = spell.as_str()),
            }),
        };
    }
}
