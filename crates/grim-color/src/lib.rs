//! GRIM colour markup and its rendering to ANSI.
//!
//! Two markup families, both transport-independent:
//! - 16-colour codes: `{` + one char (`{r`, `{R`, `{x`, numeric/symbol aliases).
//! - 24-bit hex codes: `@x` / `@b` + three hex digits, and `@r` reset.
//!
//! [`ansi`] renders markup to ANSI escape sequences for a terminal. A protocol
//! that is not a terminal (WebSocket) passes the markup through untouched.
//!
//! [`convert_16color`] rewrites the `{`-family into the `@x`-family using GRIM's
//! palette, so all colour reaches [`ansi`] in one form. [`escape_codes`] doubles
//! the two markup introducers so an untrusted value renders literally.
//!
//! Concerns split into modules: [`render`] (markup → ANSI), [`convert`]
//! (`{`-family → `@x`-family), [`escape`] (neutralising untrusted values),
//! [`width`] (visible-column measuring / token-safe truncation), and [`palette`]
//! (the 16-colour palette values).

mod convert;
mod escape;
mod render;
mod width;

pub mod palette;

pub use convert::convert_16color;
pub use escape::escape_codes;
pub use render::ansi;
pub use width::{truncate_visible, visible_width};
