//! The setting registry: definitions plus fail-safe resolution.

use std::collections::HashMap;

use bevy::prelude::*;

/// Scope a setting applies at. Character-only today; `Account` is reserved
/// for later (ADR-0007 §4) — add the variant when the first account-wide
/// setting lands, never ahead of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Character,
}

/// One registered setting: lookup key, accepted values, fallback default.
/// Keys and values are matched case-insensitively at the edges and stored in
/// canonical form (the `valid` entry's own casing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDef {
    pub key: String,
    pub valid: Vec<String>,
    pub default: String,
    pub scope: Scope,
}

impl ConfigDef {
    /// The canonical accepted value matching `raw` (case-insensitive), if any.
    pub fn canonical(&self, raw: &str) -> Option<&str> {
        self.valid
            .iter()
            .find(|v| v.eq_ignore_ascii_case(raw))
            .map(String::as_str)
    }
}

/// Setting registry, seeded by the owning features (e.g. `minimap` by
/// `ActorPlugin`). Read through [`ConfigRegistry::resolve`].
#[derive(Resource, Debug, Default)]
pub struct ConfigRegistry {
    defs: HashMap<String, ConfigDef>,
}
impl ConfigRegistry {
    /// Register (or replace) a setting definition.
    pub fn register(&mut self, def: ConfigDef) {
        self.defs.insert(def.key.clone(), def);
    }

    /// Look up a definition by exact key. Callers normalize case first.
    pub fn get(&self, key: &str) -> Option<&ConfigDef> {
        self.defs.get(key)
    }

    /// Every definition, sorted by key — deterministic listing for `config`.
    pub fn all(&self) -> Vec<&ConfigDef> {
        let mut defs: Vec<&ConfigDef> = self.defs.values().collect();
        defs.sort_by(|a, b| a.key.cmp(&b.key));
        defs
    }

    /// Resolve the effective value: the stored choice when registered and
    /// valid, else the default. `None` only for an unregistered key. An
    /// invalid stored value falls back without writing anything back, so a
    /// hand-edited or stale value can never break the reader.
    pub fn resolve<'a>(
        &'a self,
        stored: &'a HashMap<String, String>,
        key: &str,
    ) -> Option<&'a str> {
        let def = self.defs.get(key)?;
        match stored.get(key) {
            Some(value) if def.canonical(value).is_some() => Some(value),
            _ => Some(def.default.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ConfigRegistry {
        let mut registry = ConfigRegistry::default();
        registry.register(ConfigDef {
            key: "minimap".into(),
            valid: vec!["on".into(), "off".into()],
            default: "on".into(),
            scope: Scope::Character,
        });
        registry
    }

    #[test]
    fn resolve_prefers_valid_stored_value() {
        let registry = registry();
        let stored = HashMap::from([("minimap".into(), "off".into())]);
        assert_eq!(registry.resolve(&stored, "minimap"), Some("off"));
    }

    #[test]
    fn resolve_falls_back_to_default() {
        let registry = registry();
        // Missing choice and garbage choice both resolve to the default.
        assert_eq!(registry.resolve(&HashMap::new(), "minimap"), Some("on"));
        let stored = HashMap::from([("minimap".into(), "sideways".into())]);
        assert_eq!(registry.resolve(&stored, "minimap"), Some("on"));
    }

    #[test]
    fn resolve_unknown_key_is_none() {
        assert_eq!(registry().resolve(&HashMap::new(), "frobnicate"), None);
    }

    #[test]
    fn canonical_matches_case_insensitively() {
        let def = registry().get("minimap").expect("seeded").clone();
        assert_eq!(def.canonical("OFF"), Some("off"));
        assert_eq!(def.canonical("bogus"), None);
    }

    #[test]
    fn all_lists_sorted_by_key() {
        let mut registry = registry();
        registry.register(ConfigDef {
            key: "brief".into(),
            valid: vec!["on".into(), "off".into()],
            default: "off".into(),
            scope: Scope::Character,
        });
        let keys: Vec<&str> = registry.all().iter().map(|d| d.key.as_str()).collect();
        assert_eq!(keys, vec!["brief", "minimap"]);
    }
}
