//! Ground placement: the shared "what lies in this room" vocabulary.
//!
//! Ground objects carry the actor layer's [`InRoom`] for their room; carried
//! ones carry [`CarriedBy`] instead — never both. The canonical [`Ground`]
//! alias pins that bound, and [`ground_in`] funnels every ground query
//! through one definition.

use bevy::prelude::*;
use grim_actor::InRoom;
use grim_core::components::{Keywords, Name as GrimName};

use crate::object::{CarriedBy, Object};

/// Ground objects: marker, room placement, no carrier. Carried objects match
/// neither bound (`InRoom` is required), so they are invisible to `get` even
/// when the holder stands in the room.
pub type Ground<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static InRoom,
        &'static GrimName,
        Option<&'static Keywords>,
    ),
    (With<Object>, Without<CarriedBy>),
>;

/// Every ground object in `room`.
pub fn ground_in(room: Entity, objects: &Ground) -> Vec<Entity> {
    objects
        .iter()
        .filter(|(_, ir, _, _)| ir.room == room)
        .map(|(e, ..)| e)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Message)]
    struct Probe {
        room: Entity,
    }

    #[derive(Message, Clone, PartialEq, Debug)]
    struct ProbeHit(Vec<Entity>);

    fn probe(mut events: MessageReader<Probe>, objects: Ground, mut out: MessageWriter<ProbeHit>) {
        for ev in events.read() {
            out.write(ProbeHit(ground_in(ev.room, &objects)));
        }
    }

    #[test]
    fn ground_in_skips_carried_and_far_rooms() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<Probe>().add_message::<ProbeHit>();
        app.add_systems(Update, probe);
        let room = app.world_mut().spawn_empty().id();
        let elsewhere = app.world_mut().spawn_empty().id();
        let ground = app
            .world_mut()
            .spawn((Object, GrimName("coin".into()), InRoom { room }))
            .id();
        let carried = app
            .world_mut()
            .spawn((
                Object,
                GrimName("coin".into()),
                CarriedBy {
                    carrier: Entity::PLACEHOLDER,
                },
            ))
            .id();
        let far = app
            .world_mut()
            .spawn((Object, GrimName("coin".into()), InRoom { room: elsewhere }))
            .id();
        app.world_mut().write_message(Probe { room });
        app.update();
        let msgs = app.world().resource::<Messages<ProbeHit>>();
        let mut cursor = msgs.get_cursor();
        let got: Vec<Entity> = cursor
            .read(msgs)
            .flat_map(|h| h.0.iter().copied())
            .collect();
        assert_eq!(got, vec![ground]);
        assert!(!got.contains(&carried));
        assert!(!got.contains(&far));
    }
}
