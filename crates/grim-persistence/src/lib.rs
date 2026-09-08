//! Persistence: load accounts on startup + characters lazily on login, save on
//! disconnect/quit/move.

pub mod bans;
pub mod persistence;

pub use bans::{BanEntry, BanList};
pub use persistence::{
    load_account_characters, load_all_characters, load_character_by_name, PersistenceConfig,
    PersistencePlugin,
};
