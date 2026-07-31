//! [`GrimId`] — the immutable identity of a persistent record (account,
//! character, and a Blueprint's area/rooms).
//!
//! A base62 (`A-Za-z0-9`) `nanoid` of length 12 (~71 bits) — see
//! `docs/adr/0001-area-identity-and-instancing.md`. The base62 alphabet has no
//! `-`/`_`, so a Grim ID never shares a shape with a **Slug** (which uses
//! hyphens). Generated once at creation and never changed.
//!
//! Backed by a fixed `[u8; 12]` so the type is `Copy` (ids are passed around
//! constantly). It therefore accepts ONLY a 12-char base62 string on the wire —
//! records written with the old UUID ids must be migrated first (see
//! `scripts/migrate_ids_to_grimid.py`).

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Base62 alphabet: `A-Za-z0-9`. No `-`/`_`, so a Grim ID is shape-distinct
/// from a Slug.
const ALPHABET: [char; 62] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', //
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z', //
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z',
];

/// Length of a Grim ID.
pub const LEN: usize = 12;

/// A stable, immutable record identity (see module docs).
///
/// `Copy` — the bytes are always 12 ASCII base62 characters.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GrimId([u8; LEN]);

impl GrimId {
    /// Generate a fresh Grim ID (base62, length 12).
    pub fn new() -> Self {
        let s = nanoid::nanoid!(LEN, &ALPHABET);
        let mut bytes = [0u8; LEN];
        bytes.copy_from_slice(s.as_bytes());
        Self(bytes)
    }

    /// Parse a Grim ID from a string: exactly [`LEN`] base62 characters.
    pub fn parse(s: &str) -> Result<Self, String> {
        if s.len() != LEN || !s.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(format!(
                "invalid GrimId {s:?}: must be {LEN} base62 (A-Za-z0-9) characters"
            ));
        }
        let mut bytes = [0u8; LEN];
        bytes.copy_from_slice(s.as_bytes());
        Ok(Self(bytes))
    }

    /// The id as a string slice.
    pub fn as_str(&self) -> &str {
        // Safe: only ever constructed from base62 ASCII.
        std::str::from_utf8(&self.0).expect("GrimId is always valid ASCII")
    }
}

impl Default for GrimId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for GrimId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for GrimId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GrimId({})", self.as_str())
    }
}

impl Serialize for GrimId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for GrimId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        GrimId::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_base62_and_len_12() {
        let id = GrimId::new();
        assert_eq!(id.as_str().len(), LEN);
        assert!(id.as_str().chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(!id.as_str().contains('-') && !id.as_str().contains('_'));
    }

    #[test]
    fn new_ids_are_unique() {
        assert_ne!(GrimId::new(), GrimId::new());
    }

    #[test]
    fn is_copy() {
        // Compiles only if GrimId: Copy (no move on the first use).
        let a = GrimId::new();
        let b = a;
        let _ = a;
        let _ = b;
    }

    #[test]
    fn parse_round_trips_and_serializes_as_bare_string() {
        let id = GrimId::parse("abcDEF123456").unwrap();
        assert_eq!(id.to_string(), "abcDEF123456");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"abcDEF123456\"");
        let back: GrimId = serde_json::from_str("\"abcDEF123456\"").unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn parse_rejects_wrong_length_and_non_base62() {
        assert!(GrimId::parse("short").is_err());
        assert!(GrimId::parse("way-too-long-for-a-grim-id").is_err());
        // A 36-char UUID is rejected — must be migrated first.
        assert!(GrimId::parse("bc356928-d0db-41a6-abae-4d10c7b28834").is_err());
        // Right length, wrong alphabet (hyphen).
        assert!(GrimId::parse("abc-DEF12345").is_err());
    }
}
