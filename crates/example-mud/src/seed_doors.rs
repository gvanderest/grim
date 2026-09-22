//! Door blueprint wiring: the `doors` map on room blueprints.
//!
//! Split from `seed` (file-length cap): the door shape plus its resolver.

use std::collections::HashMap;

use bevy::log::warn;
use bevy::prelude::*;
use grim::prelude::{Cardinal, Door};
use serde::Deserialize;

/// A door hung on one side of an exit: display name, look keywords, and
/// whether passage starts allowed. The far side mirrors its own copy.
#[derive(Deserialize)]
pub(crate) struct DoorBlueprint {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) keywords: Vec<String>,
    #[serde(default)]
    pub(crate) open: bool,
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
            },
        );
    }
    resolved
}
