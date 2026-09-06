//! `grim-object`: things beings can pick up, carry, and drop.
//!
//! An object is any entity carrying the [`Object`] marker plus the shared
//! descriptive components (`Name` = short name, `Keywords`, `RoomDescription` =
//! long room line, optional `Description` look paragraphs). Placement reuses
//! the actor layer's [`InRoom`]: an object on the ground carries `InRoom` for
//! its room, a carried one carries [`CarriedBy`] for its carrier instead —
//! never both, so room listings (which filter on `InRoom`) never show carried
//!
//! Entity composition: ground = `Object + Name + Keywords + RoomDescription +
//! InRoom`; carried = the same minus `InRoom`, plus `CarriedBy`.
//!
//! It depends on `grim-actor` (for `InRoom` and the shared target ranking) and
//! never the reverse: beings never know about things.
//!
//! Carried objects snapshot whole into the character file on save and re-spawn
//! on login (`persist`); ground objects regenerate from area blueprints.
pub mod commands;
pub mod object;
pub mod persist;
pub mod plugin;

pub use object::{CarriedBy, Object};
pub use plugin::ObjectPlugin;
