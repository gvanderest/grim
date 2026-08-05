//! Character build data: the `Gender` enum a MUD author draws from at creation.
//!
//! The playable race/class catalogues (`RaceDef` / `ClassDef` and their
//! registries) are *content*, not core types, and live in `grim-world`.

use serde::{Deserialize, Serialize};

/// A character's gender. A closed set (unlike race/class, which are data), so a
/// plugin can exhaustively match it. Serialized lowercase (`"male"`) exactly
/// like [`crate::components::Role`], so the character JSON stays human-editable.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Gender {
    Male,
    Female,
    /// The default — a character created before genders existed, or one that
    /// declined to pick, is neutral.
    #[default]
    Neutral,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Gender serde ─────────────────────────────────────────────

    #[test]
    fn gender_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Gender::Male).unwrap(), "\"male\"");
        assert_eq!(
            serde_json::to_string(&Gender::Female).unwrap(),
            "\"female\""
        );
        assert_eq!(
            serde_json::to_string(&Gender::Neutral).unwrap(),
            "\"neutral\""
        );
    }

    #[test]
    fn gender_deserializes_lowercase() {
        assert_eq!(
            serde_json::from_str::<Gender>("\"male\"").unwrap(),
            Gender::Male
        );
        assert_eq!(
            serde_json::from_str::<Gender>("\"neutral\"").unwrap(),
            Gender::Neutral
        );
    }

    #[test]
    fn gender_default_is_neutral() {
        assert_eq!(Gender::default(), Gender::Neutral);
    }
}
