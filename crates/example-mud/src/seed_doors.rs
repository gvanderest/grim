//! Door blueprint wiring: the `doors` map on room blueprints.
//!
//! Split from `seed` (file-length cap): the door shape plus its resolver.

use std::collections::HashMap;

use bevy::log::warn;
use bevy::prelude::*;
use grim::prelude::{Cardinal, Door};
use serde::Deserialize;

/// A door hung on one side of an exit: display name, look keywords, whether
/// passage starts allowed, and whether the exit is hidden from this side.
/// The far side mirrors its own copy (`hidden` need not match — a one-way
/// secret is hidden on one side only).
#[derive(Deserialize)]
pub(crate) struct DoorBlueprint {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) keywords: Vec<String>,
    #[serde(default)]
    pub(crate) open: bool,
    #[serde(default)]
    pub(crate) hidden: bool,
}

/// Resolve one room's doors (direction name -> door blueprint) to the live
/// `Doors` map. Bad directions log and skip, mirroring `resolve_exits`.
pub(crate) fn resolve_doors(
    area_slug: &str,
    room_slug: &str,
    doors: &HashMap<String, DoorBlueprint>,
) -> HashMap<Cardinal, Door> {
    let mut resolved = HashMap::new();
    for (dir, door) in doors {
        let Some(cardinal) = Cardinal::parse(dir) else {
            warn!("area '{area_slug}' room '{room_slug}': bad door direction '{dir}'");
            continue;
        };
        resolved.insert(
            cardinal,
            Door {
                name: door.name.clone(),
                keywords: door.keywords.clone(),
                open: door.open,
                hidden: door.hidden,
            },
        );
    }
    resolved
}
