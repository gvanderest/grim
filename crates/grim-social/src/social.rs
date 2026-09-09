//! Social definitions: the seven-string schema, the registry, and the disk
//! loader. File overrides layer per-variant over catalog defaults — a file
//! holding only two keys reskins those two wordings and inherits the rest.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;

/// Directory of `data/socials/<name>.json` overrides, resolved against the
/// process working directory when relative (`/opt/grim` in production, where
/// the deploy ships the `data/socials` folder alongside the binary).
/// Insert this resource before [`crate::SocialPlugin`] runs to override the
/// default (`data/socials`).
#[derive(Resource, Clone, Debug)]
pub struct SocialDir(pub PathBuf);

impl Default for SocialDir {
    fn default() -> Self {
        Self(PathBuf::from("data/socials"))
    }
}

/// The built-in social set, used when no file overrides them. The engine
/// works out of the box with zero social files present.
pub const BUILTIN_SOCIALS: &[&str] = &[
    "grin", "smile", "wave", "laugh", "nod", "bow", "chuckle", "cry", "dance", "hug", "kiss",
    "shrug", "sigh", "wink",
];

/// One file-held case with actor + room wordings (`solo`, `self_target`).
/// Every field is optional: a missing key inherits the built-in default.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct CasePair {
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub room: Option<String>,
}

/// The file-held `with_target` case: actor, target, and room wordings.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct OtherCase {
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub room: Option<String>,
}

/// One `data/socials/<name>.json` file: the three skinnable cases. Filename
/// (minus `.json`, lowercased) is the command name, so builders add a verb by
/// copying a file.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct SocialFile {
    #[serde(default)]
    pub solo: CasePair,
    #[serde(default)]
    pub self_target: CasePair,
    #[serde(default)]
    pub with_target: OtherCase,
}

/// A social's runtime definition: its name plus any file-held overrides.
/// A `None` override renders the catalog default (`social.<name>.<case>.
/// <audience>`); a `Some` renders the file text through the same substitution
#[derive(Debug, Clone)]
pub struct SocialDef {
    /// Lowercase command name (`grin`).
    pub name: String,
    /// File-held overrides (the future in-game editor fills these).
    pub overrides: SocialFile,
}

impl SocialDef {
    /// Template for one wording: the file override when present, else the
    /// catalog default. `case` is `solo`/`self`/`other`, `audience` is
    /// `actor`/`room`/`target`. Unknown pairs fall back to the key itself
    /// (the catalog's missing-entry behavior).
    pub fn template(&self, case: &str, audience: &str) -> String {
        let held = match (case, audience) {
            ("solo", "actor") => self.overrides.solo.actor.clone(),
            ("solo", "room") => self.overrides.solo.room.clone(),
            ("self", "actor") => self.overrides.self_target.actor.clone(),
            ("self", "room") => self.overrides.self_target.room.clone(),
            ("other", "actor") => self.overrides.with_target.actor.clone(),
            ("other", "target") => self.overrides.with_target.target.clone(),
            ("other", "room") => self.overrides.with_target.room.clone(),
            _ => None,
        };
        held.unwrap_or_else(|| grim_text::tr(&self.key(case, audience), &[]))
    }

    fn key(&self, case: &str, audience: &str) -> String {
        format!("social.{}.{}.{}", self.name, case, audience)
    }
}

/// All loaded socials, by lowercase name.
#[derive(Resource, Debug, Default)]
pub struct SocialRegistry {
    defs: HashMap<String, SocialDef>,
}

impl SocialRegistry {
    /// Look up a social by command name (case-insensitive). A miss means the
    /// name was never loaded — the handler answers unknown-command, never
    /// panics.
    pub fn get(&self, name: &str) -> Option<&SocialDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    /// Add or replace a definition (a file load, or the future in-game
    /// editor). Keyed by the definition's own lowercase name.
    pub fn insert(&mut self, def: SocialDef) {
        self.defs.insert(def.name.clone(), def);
    }

    /// Every loaded name, for startup command registration.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.defs.keys().map(String::as_str)
    }
}

impl SocialDef {
    /// A definition with no file overrides (renders catalog defaults). The
    /// future in-game editor builds one of these, fills overrides, and calls
    /// [`SocialRegistry::insert`].
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into().to_ascii_lowercase();
        Self {
            name,
            overrides: SocialFile::default(),
        }
    }
}

/// Load the registry: built-ins first, then every `*.json` file in `dir`
/// overlaid (new names included — copying a file adds a verb). A missing dir
/// means built-ins only; an unreadable or unparsable file is logged and
/// skipped, never fatal.
pub fn load_socials(dir: &Path) -> SocialRegistry {
    let mut registry = SocialRegistry::default();
    for name in BUILTIN_SOCIALS {
        registry.defs.insert(
            name.to_string(),
            SocialDef {
                name: name.to_string(),
                overrides: SocialFile::default(),
            },
        );
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return registry,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if stem.is_empty() || stem.split_whitespace().count() != 1 {
            warn!(
                "social: skipping '{}': not a single-word command name",
                path.display()
            );
            continue;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                warn!("social: skipping '{}': {e}", path.display());
                continue;
            }
        };
        match serde_json::from_str::<SocialFile>(&text) {
            Ok(file) => {
                registry.defs.insert(
                    stem.clone(),
                    SocialDef {
                        name: stem,
                        overrides: file,
                    },
                );
            }
            Err(e) => warn!("social: skipping '{}': {e}", path.display()),
        }
    }
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_names_load_with_no_dir() {
        let registry = load_socials(Path::new("no-such-dir"));
        assert_eq!(registry.defs.len(), BUILTIN_SOCIALS.len());
        assert!(registry.get("grin").is_some());
        assert!(registry.get("GRIN").is_some());
        assert!(registry.get("xyzzy").is_none());
    }

    #[test]
    fn default_template_resolves_catalog() {
        let registry = load_socials(Path::new("no-such-dir"));
        let grin = registry.get("grin").unwrap();
        assert_eq!(grin.template("solo", "actor"), "You grin.\n");
        assert_eq!(
            grin.template("other", "room"),
            "%{actor} grins at %{target}.\n"
        );
    }

    fn fixture_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("grim-social-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn file_override_wins_per_variant() {
        let dir = fixture_dir("override");
        std::fs::write(
            dir.join("grin.json"),
            r#"{"solo": {"actor": "You beam.\n"}}"#,
        )
        .unwrap();
        let registry = load_socials(&dir);
        let grin = registry.get("grin").unwrap();
        assert_eq!(grin.template("solo", "actor"), "You beam.\n");
        assert_eq!(grin.template("solo", "room"), "%{actor} grins.\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_file_adds_a_verb() {
        let dir = fixture_dir("newverb");
        std::fs::write(
            dir.join("smirk.json"),
            r#"{"solo": {"actor": "You smirk.\n"}}"#,
        )
        .unwrap();
        let registry = load_socials(&dir);
        assert!(registry.get("smirk").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_file_and_bad_name_are_skipped() {
        let dir = fixture_dir("bad");
        std::fs::write(dir.join("grin.json"), r#"{"solo": {"#).unwrap();
        std::fs::write(dir.join("two words.json"), "{}").unwrap();
        std::fs::write(dir.join("notes.txt"), "{}").unwrap();
        let registry = load_socials(&dir);
        assert_eq!(
            registry.get("grin").unwrap().template("solo", "actor"),
            "You grin.\n"
        );
        assert!(registry.get("two words").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
