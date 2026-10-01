//! Death: mob corpses, PC respawn, corpse timers.
//!
//! `kill_being` is the single funnel every death path routes through
//! (immediate kill strikes, round ticks, kick/cast kills).

use bevy::prelude::*;
use grim_actor::{Character, Health, InRoom, Posture};
use grim_core::components::{Keywords, Name as GrimName, RoomDescription};
use grim_core::events::InfoMessage;
use grim_object::{CarriedBy, Object};
use grim_text::tr;
use grim_world::{Area, Room};

use crate::corpse::Corpse;
use crate::engage::strip;
use crate::events::{DamageKind, Died};
use crate::formula::{coin_for_level, death_debt, xp_for_level};
use crate::loot::GlobalLoot;

/// The corpse despawns this many seconds after death, whatever remains.
pub const CORPSE_TIMER: f32 = 120.0;

/// Kill `victim` in `room`, crediting `killer`. Mob → container-corpse filled
/// from the global table (+ killer XP/coin). PC → respawn at the starting
/// room, full health, inventory kept, XP debt. Emits [`Died`] either way.
pub fn kill_being(world: &mut World, victim: Entity, killer: Option<Entity>, room: Entity) {
    let victim_name = world
        .get::<GrimName>(victim)
        .map(|n| n.0.clone())
        .unwrap_or_else(|| "Someone".into());
    let victim_pc = world.get::<Character>(victim).is_some();
    let victim_level = world
        .get::<grim_actor::Actor>(victim)
        .map(|a| a.level)
        .unwrap_or(1);
    let (killer_name, killer_pc) = match killer {
        Some(k) => (
            world.get::<GrimName>(k).map(|n| n.0.clone()),
            world.get::<Character>(k).is_some(),
        ),
        None => (None, false),
    };
    if victim_pc {
        kill_pc(
            world,
            victim,
            &victim_name,
            killer,
            killer_name.clone(),
            room,
        );
        world.resource_mut::<Messages<Died>>().write(Died {
            victim,
            victim_name,
            killer,
            killer_name,
            room,
            victim_pc: true,
            killer_pc,
            xp: 0,
            coin: 0,
        });
    } else {
        let (xp, coin) = fill_corpse(world, victim, &victim_name, killer, room, victim_level);
        world.resource_mut::<Messages<Died>>().write(Died {
            victim,
            victim_name,
            killer,
            killer_name,
            room,
            victim_pc: false,
            killer_pc,
            xp,
            coin,
        });
    }
}

/// PC death: strip engagement, full heal, respawn at the starting room,
/// posture standing, XP debt saturating. Inventory and coin untouched.
fn kill_pc(
    world: &mut World,
    victim: Entity,
    victim_name: &str,
    _killer: Option<Entity>,
    _killer_name: Option<String>,
    _room: Entity,
) {
    strip(world, victim);
    if let Some(mut h) = world.get_mut::<Health>(victim) {
        *h = Health::pc();
    }
    if let Some(mut p) = world.get_mut::<Posture>(victim) {
        *p = Posture::Standing;
    }
    let level = world
        .get::<grim_actor::Actor>(victim)
        .map(|a| a.level)
        .unwrap_or(1);
    if let Some(mut ch) = world.get_mut::<Character>(victim) {
        ch.xp = ch.xp.saturating_sub(death_debt(level));
    }
    // Respawn at the starting room.
    let starting = world
        .get_resource::<grim_world::StartingRoom>()
        .map(|s| s.0);
    if let Some(dest) = starting {
        if let Some(mut ir) = world.get_mut::<InRoom>(victim) {
            ir.room = dest;
        }
        let loc = {
            let room = world
                .get::<Room>(dest)
                .map(|r| (r.friendly_id.clone(), r.area));
            room.and_then(|(room_slug, area_e)| {
                world.get::<Area>(area_e).map(|a| grim_world::RoomLocation {
                    area: a.friendly_id.clone(),
                    room: room_slug,
                })
            })
        };
        if let Some(loc) = loc {
            if let Some(mut ch) = world.get_mut::<Character>(victim) {
                ch.last_room = Some(loc);
            }
        }
    }
    world
        .resource_mut::<Messages<InfoMessage>>()
        .write(InfoMessage {
            target: victim,
            text: tr!("combat.death.pc", name = victim_name),
        });
}

/// Mob death: spawn the container-corpse, roll the global table into it,
/// strip + despawn the mob, bank killer XP/coin. Returns `(xp, coin)`.
fn fill_corpse(
    world: &mut World,
    victim: Entity,
    victim_name: &str,
    killer: Option<Entity>,
    room: Entity,
    victim_level: u32,
) -> (u32, u32) {
    let xp = xp_for_level(victim_level);
    let coin = coin_for_level(victim_level);
    // Roll loot first (needs &mut rng), then spawn.
    let mut drops: Vec<(String, Vec<String>, Vec<String>, String)> = Vec::new();
    {
        let table: Vec<_> = world.resource::<GlobalLoot>().0.clone();
        let mut rng = world.resource_mut::<crate::state::CombatRng>();
        for entry in &table {
            if rng.0.below(100) < u64::from(entry.chance_pct) {
                drops.push((
                    entry.name.clone(),
                    entry.description.clone(),
                    entry.keywords.clone(),
                    entry.room_description.clone(),
                ));
            }
        }
    }
    let victim_keywords: Vec<String> = world
        .get::<Keywords>(victim)
        .map(|k| k.0.clone())
        .unwrap_or_default();
    let mut keywords = vec!["corpse".to_string()];
    keywords.extend(victim_keywords);
    let corpse = world
        .spawn((
            Corpse {
                timer: CORPSE_TIMER,
            },
            grim_object::Container,
            grim_object::OneWay,
            GrimName(format!("corpse of {victim_name}")),
            Keywords(keywords),
            RoomDescription(format!("The corpse of {victim_name} lies here.")),
            crate::corpse::CorpseMarker,
            InRoom { room },
        ))
        .id();
    for (name, description, keywords, room_desc) in drops {
        let item = world
            .spawn((
                Object,
                GrimName(name),
                grim_core::components::Description(description),
                Keywords(keywords),
                RoomDescription(room_desc),
                CarriedBy { carrier: corpse },
            ))
            .id();
        let _ = item;
    }
    strip(world, victim);
    world.despawn(victim);
    if let Some(k) = killer {
        if world.get::<Character>(k).is_none() {
            return (xp, coin);
        }
        if let Some(mut ch) = world.get_mut::<Character>(k) {
            ch.xp += xp;
            ch.coin += coin;
            let xp_text = xp.to_string();
            let coin_text = coin.to_string();
            world
                .resource_mut::<Messages<InfoMessage>>()
                .write(InfoMessage {
                    target: k,
                    text: tr!(
                        "combat.xp_gain",
                        total = xp_text.as_str(),
                        coin = coin_text.as_str()
                    ),
                });
        }
    }
    let _ = DamageKind::Strike;
    (xp, coin)
}

/// Tick corpse timers; despawn corpses whose timer expired or that hold no
/// contents. Contents despawn with the corpse (they are `CarriedBy` it).
pub fn tick_corpses(
    mut commands: Commands,
    time: Res<Time>,
    mut corpses: Query<(Entity, &mut Corpse, &InRoom)>,
    carried: Query<(Entity, &CarriedBy)>,
) {
    for (corpse, mut timer, _) in corpses.iter_mut() {
        timer.timer -= time.delta().as_secs_f32();
        let empty = !carried.iter().any(|(_, c)| c.carrier == corpse);
        if timer.timer <= 0.0 || empty {
            let contents: Vec<Entity> = carried
                .iter()
                .filter(|(_, c)| c.carrier == corpse)
                .map(|(e, _)| e)
                .collect();
            for item in contents {
                commands.entity(item).despawn();
            }
            commands.entity(corpse).despawn();
        }
    }
}
