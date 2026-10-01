//! Corpse marker + container contents helpers.
//!
//! A corpse is an entity carrying [`Corpse`] (the combat timer), the shared
//! `Container` + `OneWay` markers (owned by `grim-object`; the sibling scene
//! agent creates them — this re-exports the path contract), a `Name` of
//! `"corpse of {victim}"`, `Keywords` (`corpse` + the victim's), a
//! `RoomDescription`, and `InRoom`. Loot items are `Object` entities with
//! `CarriedBy { carrier: corpse }`.

use bevy::prelude::*;
use grim_object::CarriedBy;

/// Countdown until the corpse (and its contents) despawn.
#[derive(Component, Debug, Clone, Copy)]
pub struct Corpse {
    pub timer: f32,
}

/// Disambiguator so corpse queries never collide with plain containers.
#[derive(Component, Debug, Clone, Copy)]
pub struct CorpseMarker;

/// True when `entity` is a corpse.
pub fn is_corpse(world: &mut World, entity: Entity) -> bool {
    world.get::<Corpse>(entity).is_some()
}

/// Contents of a corpse: objects carried by it.
pub fn contents_in(corpse: Entity, carried: &Query<(Entity, &CarriedBy)>) -> Vec<Entity> {
    carried
        .iter()
        .filter(|(_, c)| c.carrier == corpse)
        .map(|(e, _)| e)
        .collect()
}
