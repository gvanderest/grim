//! Persistence: load accounts on startup + characters lazily on login, save on
//! disconnect/quit/move.

pub mod persistence;

pub use persistence::{
    load_account_characters, load_character_by_name, PersistenceConfig, PersistencePlugin,
};
