//! Ergonomic macros over the catalog lookup. Kept out of `lib.rs` so the crate
//! root stays declarations + re-exports only (the no-definitions rule enforced
//! by `scripts/check-file-length.sh`). `#[macro_export]` still surfaces `tr!`
//! at the crate root, so `grim_text::tr!` / `$crate::tr` paths are unchanged.

/// Look up and interpolate a catalog key with `t!()`-style syntax.
///
/// ```ignore
/// tr!("login.prompt");
/// tr!("social.say.third_party", speaker = speaker, text = text);
/// ```
#[macro_export]
macro_rules! tr {
    ($key:expr $(, $arg:ident = $val:expr)* $(,)?) => {{
        let args: &[(&str, &str)] = &[$((stringify!($arg), $val.as_ref())),*];
        $crate::tr($key, args)
    }};
}
