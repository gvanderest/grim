//! Container placement: the shared "what holds what" vocabulary.
//!
//! A container is a ground [`Object`] carrying [`Container`] (and maybe
//! [`OneWay`]) plus `InRoom`; its contents are `Object`s carrying
//! [`CarriedBy`] for the container. The [`Boxes`] alias pins that bound, and
//! [`box_in`] funnels every container lookup through one definition.

use bevy::prelude::*;
use grim_actor::InRoom;
use grim_core::components::{Keywords, Name as GrimName};

use crate::object::{CarriedBy, Container, Object};

/// Ground containers: object marker + container marker, placed in a room,
/// never carried.
pub type Boxes<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GrimName,
        Option<&'static Keywords>,
        &'static InRoom,
    ),
    (With<Object>, With<Container>, Without<CarriedBy>),
>;

/// Ground containers with their one-way flag: corpses refuse `put`.
pub type BoxesOneWay<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GrimName,
        Option<&'static Keywords>,
        &'static InRoom,
        Option<&'static crate::object::OneWay>,
    ),
    (With<Object>, With<Container>, Without<CarriedBy>),
>;

/// Every ground container in `room`.
pub fn box_in(room: Entity, objects: &Boxes) -> Vec<Entity> {
    objects
        .iter()
        .filter(|(_, _, _, ir)| ir.room == room)
        .map(|(e, ..)| e)
        .collect()
}
