//! Persistence: load accounts/characters on startup, save on disconnect.

pub mod persistence;

pub use persistence::{PersistenceConfig, PersistencePlugin};
