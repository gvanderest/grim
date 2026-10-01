//! NPC stamping: spawn one mob blueprint into a room.
//!
//! Split from `seed` (file cap): the being bundle (level/HP/posture/noun,
//! aggression, script triggers) lives here; area stamping stays there.

use bevy::log::{error, info};
use bevy::prelude::*;
use grim::components::Name as GrimName;
use grim::prelude::{
    compile, Actor, CompiledTrigger, Creature, Description, Gender, InRoom, Keywords,
    RoomDescription, ScriptTriggers,
};

use crate::seed::NpcBlueprint;

/// Stamp one NPC blueprint into `room`: the being bundle plus its compiled
/// script triggers. A trigger that fails to compile is logged and skipped —
/// the mob still spawns, triggerless for that moment.
pub(crate) fn spawn_npc(
    commands: &mut Commands,
    area_slug: &str,
    npc: &NpcBlueprint,
    room: Entity,
) {
    // Compile each trigger once now: a typo fails loudly at startup (logged,
    // trigger skipped) instead of on a player's move.
    let mut compiled = Vec::with_capacity(npc.triggers.len());
    for def in &npc.triggers {
        match compile(&def.script) {
            Ok(bytecode) => compiled.push(CompiledTrigger {
                on: def.on,
                bytecode,
            }),
            Err(error) => error!(
                "area '{area_slug}' npc '{}': skipping trigger that does not compile: {error}",
                npc.name
            ),
        }
    }
    let mut mob = commands.spawn((
        Creature,
        // Blueprint level/HP; race/build data still unseeded (empty race,
        // neutral gender).
        Actor {
            race: String::new(),
            level: npc.level,
            gender: Gender::Neutral,
        },
        grim::Health::full(npc.health),
        grim::Posture::Standing,
        grim::AttackNoun(npc.attack_noun.clone()),
        GrimName(npc.name.clone()),
        Description(npc.description.clone()),
        Keywords(npc.keywords.clone()),
        RoomDescription(npc.room_description.clone()),
        InRoom { room },
    ));
    if npc.aggressive {
        mob.insert(grim::Aggressive);
    }
    if !compiled.is_empty() {
        info!(
            "area '{area_slug}' npc '{}': {} script trigger(s) loaded",
            npc.name,
            compiled.len()
        );
        mob.insert(ScriptTriggers(compiled));
    }
}
