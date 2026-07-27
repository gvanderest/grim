use std::collections::HashMap;

use bevy::prelude::*;
use uuid::Uuid;

use grim::prelude::{
    Area, Cardinal, Description, Exits, InRoom, Name as GrimName, Npc, Room, StartingRoom,
};

/// Seed the initial world state — areas, rooms, exits, NPCs.
/// Called once at startup.
pub fn seed_world(mut commands: Commands) {
    // ── Haven area ───────────────────────────────────────────────────
    let haven = commands
        .spawn(Area {
            id: Uuid::new_v4(),
            friendly_id: "haven".to_string(),
            name: "Haven".to_string(),
        })
        .id();

    // ── Rooms ────────────────────────────────────────────────────────
    let tavern = commands
        .spawn((
            Room {
                id: Uuid::new_v4(),
                friendly_id: "tavern".to_string(),
                name: "The Rusted Anvil".to_string(),
                description: "Smoke and the clang of hammer on steel fill the air. \
                    Sawdust covers the floor of this cozy tavern. A staircase leads up \
                    to the second floor, and a door leads north to the town square."
                    .to_string(),
                area: haven,
            },
            GrimName("The Rusted Anvil".to_string()),
            Exits::default(),
        ))
        .id();
    let square = commands
        .spawn((
            Room {
                id: Uuid::new_v4(),
                friendly_id: "square".to_string(),
                name: "Town Square".to_string(),
                description: "The cobblestone square bustles with activity. A fountain \
                    marks the center. The tavern is to the south, and the smithy is to \
                    the east."
                    .to_string(),
                area: haven,
            },
            GrimName("Town Square".to_string()),
            Exits::default(),
        ))
        .id();
    let forge = commands
        .spawn((
            Room {
                id: Uuid::new_v4(),
                friendly_id: "forge".to_string(),
                name: "Grimmok's Forge".to_string(),
                description: "The heat from the forge is intense. Weapons and armor \
                    hang from the walls. The town square is to the west."
                    .to_string(),
                area: haven,
            },
            GrimName("Grimmok's Forge".to_string()),
            Exits::default(),
        ))
        .id();

    // ── Exits (bidirectional) ────────────────────────────────────────
    let mut tavern_exits = HashMap::new();
    tavern_exits.insert(Cardinal::North, square);

    let mut square_exits = HashMap::new();
    square_exits.insert(Cardinal::South, tavern);
    square_exits.insert(Cardinal::East, forge);

    let mut forge_exits = HashMap::new();
    forge_exits.insert(Cardinal::West, square);

    commands.entity(tavern).insert(Exits {
        exits: tavern_exits,
    });
    commands.entity(square).insert(Exits {
        exits: square_exits,
    });
    commands.entity(forge).insert(Exits {
        exits: forge_exits,
    });

    // ── NPC: Grimmok Ironhand, in the tavern ────────────────────────
    commands.spawn((
        Npc,
        GrimName("Grimmok Ironhand".to_string()),
        Description(
            "A burly dwarf with a scorched beard and arms thick as tree trunks. He \
             hammers rhythmically at a glowing blade, seemingly unaware of your \
             presence."
                .to_string(),
        ),
        InRoom { room: tavern },
    ));

    // ── Starting room resource ───────────────────────────────────────
    commands.insert_resource(StartingRoom(tavern));
}
