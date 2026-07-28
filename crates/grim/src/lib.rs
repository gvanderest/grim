#[macro_use]
extern crate rust_i18n;
rust_i18n::i18n!("../../locales");

pub mod cardinal;
pub mod color;
pub mod components;
pub mod events;
pub mod plugins;
pub mod prelude;
pub mod validation;
