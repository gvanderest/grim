//! Sandboxed trigger execution: one fresh Lua state per firing.
//!
//! The sandbox is structural, not advisory. Each firing builds a state with
//! only pure-data stdlib (`math`, `string`, `table`, `utf8`), then installs
//! exactly three globals: `rand()` (`[0, 1)`), `self` (the observing entity —
//! `self.say(text)` speaks as the mob), and the `event` table (`{type}` plus
//! its `deny()` method, which blocks the attempted action). Dot or colon
//! calls both work for the methods. Calling `event.deny()` composes with speech, so
//! a script says its refusal *then* denies. Lua's base library ships with
//! every state, so the entries that load code, touch
//! the host, or talk anywhere ([`STRIPPED_GLOBALS`]) are explicitly nilled —
//! the tests pin each one absent. A memory cap plus an instruction budget
//! turn runaway scripts into errors instead of hangs.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mlua::{HookTriggers, Lua, LuaOptions, StdLib};

use crate::trigger::TriggerKind;

/// Base-library globals nilled on every firing. The base library always ships
/// with the state, so the closed allow-list is enforced by removal: code
/// loading (`load`, `loadfile`, `dofile`, `require`), host access (`os`,
/// `io`, `package`, `debug`), chatter (`print`, `warn`, `collectgarbage`),
/// and suspension (`coroutine`, unused by design). What stays is pure:
/// `assert`, `error`, `pcall`, `type`, `tonumber`, `tostring`, iteration, and
/// the metatable/raw accessors — none of which can reach outside the state.
const STRIPPED_GLOBALS: &[&str] = &[
    "load",
    "loadfile",
    "dofile",
    "require",
    "print",
    "warn",
    "collectgarbage",
    "os",
    "io",
    "package",
    "debug",
    "coroutine",
];

/// Bytes a firing may allocate before it fails with a memory error. Scripts
/// build at most a few strings; anything near this is a runaway table.
const MEMORY_LIMIT_BYTES: usize = 256 * 1024;

/// VM instructions a firing may execute before the hook aborts it. A greeting
/// script runs dozens; an infinite loop trips this instead of the tick.
const INSTRUCTION_BUDGET: u32 = 100_000;

/// What one trigger firing did: every `say`ed line, plus whether the script
/// called `deny()`. Denial composes with speech — a script says its refusal
/// *then* denies, so the mover hears the echo and stays put.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Every `say`ed line, in order.
    pub said: Vec<String>,
    /// Whether the script denied the attempted action.
    pub denied: bool,
}

/// Run precompiled `bytecode` for `on`, returning what it said and whether it
/// denied.
///
/// Any Lua failure — a runtime error, an exhausted budget — comes back as
/// `Err` text. Nothing throws across this boundary: a failing script can
/// never crash the calling system.
pub fn run_trigger(bytecode: &[u8], on: TriggerKind) -> Result<Outcome, String> {
    let libs = StdLib::MATH | StdLib::STRING | StdLib::TABLE | StdLib::UTF8;
    let lua = Lua::new_with(libs, LuaOptions::default()).map_err(|e| e.to_string())?;
    lua.set_memory_limit(MEMORY_LIMIT_BYTES)
        .map_err(|e| e.to_string())?;
    lua.set_hook(
        HookTriggers {
            every_nth_instruction: Some(INSTRUCTION_BUDGET),
            ..HookTriggers::new()
        },
        |_, _| {
            Err(mlua::Error::RuntimeError(
                "instruction budget exceeded".into(),
            ))
        },
    )
    .map_err(|e| e.to_string())?;
    for name in STRIPPED_GLOBALS {
        lua.globals()
            .set(*name, mlua::Value::Nil)
            .map_err(|e| e.to_string())?;
    }
    let said: Rc<RefCell<Vec<String>>> = Rc::default();
    let denied = Rc::new(Cell::new(false));
    lua.globals()
        .set(
            "rand",
            lua.create_function(|_, ()| Ok(rand::random::<f64>()))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    // `self` is the observing entity: `self.say(text)` speaks as the mob.
    // Dot or colon call both work — the colon-passed `self` is accepted and
    // ignored, and the first string argument is the line.
    let this = lua.create_table().map_err(|e| e.to_string())?;
    this.set(
        "say",
        lua.create_function({
            let said = said.clone();
            move |_, args: mlua::MultiValue| {
                let mut text = None;
                for value in args {
                    if let mlua::Value::String(s) = value {
                        text = Some(s.to_str()?.to_string());
                        break;
                    }
                }
                match text {
                    Some(text) => {
                        said.borrow_mut().push(text);
                        Ok(())
                    }
                    None => Err(mlua::Error::RuntimeError("say() needs a text argument".into())),
                }
            }
        })
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    lua.globals()
        .set("self", this)
        .map_err(|e| e.to_string())?;
    let event = lua.create_table().map_err(|e| e.to_string())?;
    event.set("type", on.as_str()).map_err(|e| e.to_string())?;
    // `event.deny()` (or `event:deny()` — colon passes `event` as self, which
    // is accepted and ignored): blocks the attempted action. A method on the
    // event, like `self.say`, so the global surface stays `rand`/`self`/`event`.
    event
        .set(
            "deny",
            lua.create_function({
                let denied = denied.clone();
                move |_, _: mlua::MultiValue| {
                    denied.set(true);
                    Ok(())
                }
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    lua.globals()
        .set("event", event)
        .map_err(|e| e.to_string())?;
    lua.load(bytecode).exec().map_err(|e| e.to_string())?;
    let said = said.borrow().clone();
    Ok(Outcome {
        said,
        denied: denied.get(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::compile;

    fn run(source: &str, on: TriggerKind) -> Result<Outcome, String> {
        run_trigger(&compile(source).expect("fixture must compile"), on)
    }

    #[test]
    fn unconditional_say_is_captured() {
        assert_eq!(
            run("self.say('Hello there.')", TriggerKind::Enter).unwrap().said,
            ["Hello there."]
        );
    }

    #[test]
    fn chance_gate_passes_and_blocks() {
        assert_eq!(
            run("if rand() >= 0 then self.say('a') end", TriggerKind::Enter).unwrap().said,
            ["a"]
        );
        assert!(run("if rand() < 0 then self.say('a') end", TriggerKind::Enter)
            .unwrap()
            .said
            .is_empty());
    }

    #[test]
    fn rand_stays_in_unit_interval() {
        let out = run(
            "for i = 1, 200 do local r = rand() if r < 0 or r >= 1 then self.say('OUT') end end self.say('done')",
            TriggerKind::Enter,
        )
        .unwrap()
        .said;
        assert_eq!(out, ["done"]);
    }

    #[test]
    fn event_type_is_visible() {
        let out = run("self.say(event.type)", TriggerKind::AttemptLeave)
            .unwrap()
            .said;
        assert_eq!(out, ["attempt_leave"]);
    }

    #[test]
    fn multiple_says_keep_order() {
        let out = run("self.say('one') self.say('two')", TriggerKind::Leave)
            .unwrap()
            .said;
        assert_eq!(out, ["one", "two"]);
    }

    #[test]
    fn runtime_error_is_contained() {
        let err = run("nope()", TriggerKind::Enter).expect_err("nil call must fail");
        assert!(!err.is_empty());
    }

    #[test]
    fn infinite_loop_trips_budget() {
        let err = run("while true do end", TriggerKind::Enter).expect_err("loop must abort");
        assert!(err.contains("budget"), "got: {err}");
    }

    #[test]
    fn stripped_globals_stay_absent() {
        for name in STRIPPED_GLOBALS {
            let probe = format!("{name}()");
            run(&probe, TriggerKind::Enter).expect_err(&format!("{name} must stay stripped"));
        }
        // `load` with a benign chunk must fail the same way: no code loading.
        run("load('say(1)')()", TriggerKind::Enter).expect_err("load must stay stripped");
    }

    #[test]
    fn kept_base_helpers_work() {
        let out = run(
            "local t = 0 for _, v in pairs({1, 2}) do t = t + v end self.say(type('x') .. tostring(t))",
            TriggerKind::Enter,
        )
        .unwrap()
        .said;
        assert_eq!(out, ["string3"]);
    }

    #[test]
    fn allowed_pure_libraries_work() {
        let out = run(
            "self.say(string.upper('hi') .. math.floor(1.9) .. #'abc' .. utf8.len('é'))",
            TriggerKind::Enter,
        )
        .unwrap()
        .said;
        assert_eq!(out, ["HI131"], "upper + floor + len + concat");
    }

    #[test]
    fn deny_blocks_without_speech() {
        let outcome = run("event.deny()", TriggerKind::AttemptLeave).unwrap();
        assert!(outcome.denied);
        assert!(outcome.said.is_empty());
    }

    #[test]
    fn deny_composes_with_refusal_speech() {
        let outcome = run("self.say('Halt!') event:deny()", TriggerKind::AttemptEnter).unwrap();
        assert!(outcome.denied);
        assert_eq!(outcome.said, ["Halt!"]);
    }

    #[test]
    fn silence_does_not_deny() {
        let outcome = run("self.say('hi')", TriggerKind::Enter).unwrap();
        assert!(!outcome.denied);
    }

    #[test]
    fn bare_say_is_gone() {
        run("say('hi')", TriggerKind::Enter).expect_err("speech lives on self now");
    }
}

