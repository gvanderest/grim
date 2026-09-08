//! Trigger definitions: the blueprint shape and the compiled room-attachable form.
//!
//! A blueprint carries [`TriggerDef`] values (`{on, script}` with inline Lua
//! source). At spawn each source is [`compile`]d to bytecode once; the entity
//! holds [`ScriptTriggers`] so per-event firings never parse.

use bevy::prelude::*;
use mlua::{Lua, LuaOptions, StdLib};
use serde::Deserialize;

/// Which room-transition moment a script fires on.
///
/// The `Attempt*` variants fire synchronously before placement and can deny
/// the move (`deny()` latches); the committed `Enter`/`Leave` facts only
/// observe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    AttemptWalk,
    AttemptEnter,
    AttemptLeave,
    Enter,
    Leave,
}

impl TriggerKind {
    /// The `event.type` string scripts match on.
    pub fn as_str(self) -> &'static str {
        match self {
            TriggerKind::AttemptWalk => "attempt_walk",
            TriggerKind::AttemptEnter => "attempt_enter",
            TriggerKind::AttemptLeave => "attempt_leave",
            TriggerKind::Enter => "enter",
            TriggerKind::Leave => "leave",
        }
    }
}

/// One trigger as authored in a mob blueprint: the moment plus inline Lua.
#[derive(Debug, Clone, Deserialize)]
pub struct TriggerDef {
    /// Which transition moment fires this script.
    pub on: TriggerKind,
    /// Inline Lua source, run with `rand`/`say`/`event` in scope.
    pub script: String,
}

/// A trigger compiled to Lua bytecode, ready to fire.
#[derive(Debug, Clone)]
pub struct CompiledTrigger {
    /// Which transition moment fires this script.
    pub on: TriggerKind,
    /// Dumped bytecode: syntax-checked at spawn, loaded fresh per firing.
    pub bytecode: Vec<u8>,
}

/// Every script trigger on one creature, in blueprint order. All triggers
/// sharing a moment fire in this order.
#[derive(Component, Debug, Clone, Default)]
pub struct ScriptTriggers(pub Vec<CompiledTrigger>);

/// Compile Lua source to bytecode, returning the syntax error as a string.
///
/// This is the fail-early gate: blueprints compile at spawn so a typo breaks
/// loudly in the server log at startup, never mid-game on a player's move.
pub fn compile(source: &str) -> Result<Vec<u8>, String> {
    let lua = Lua::new_with(StdLib::NONE, LuaOptions::default()).map_err(|e| e.to_string())?;
    let func = lua
        .load(source)
        .into_function()
        .map_err(|e| e.to_string())?;
    Ok(func.dump(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_match_event_table() {
        assert_eq!(TriggerKind::Enter.as_str(), "enter");
        assert_eq!(TriggerKind::Leave.as_str(), "leave");
        assert_eq!(TriggerKind::AttemptEnter.as_str(), "attempt_enter");
        assert_eq!(TriggerKind::AttemptLeave.as_str(), "attempt_leave");
    }

    #[test]
    fn kind_deserializes_snake_case() {
        let def: TriggerDef =
            serde_json::from_str(r#"{"on": "attempt_leave", "script": "self.say('x')"}"#).unwrap();
        assert_eq!(def.on, TriggerKind::AttemptLeave);
        assert_eq!(def.script, "self.say('x')");
    }

    #[test]
    fn compile_accepts_valid_source() {
        let bytes = compile("if rand() < 0.1 then self.say('hi') end").unwrap();
        assert!(!bytes.is_empty(), "bytecode must not be empty");
    }

    #[test]
    fn compile_rejects_syntax_error() {
        let err = compile("if then end").expect_err("broken Lua must fail");
        assert!(
            err.contains("syntax") || err.contains("expected"),
            "got: {err}"
        );
    }

    #[test]
    fn script_triggers_default_empty() {
        assert!(ScriptTriggers::default().0.is_empty());
    }
}
