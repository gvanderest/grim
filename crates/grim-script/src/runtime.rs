//! Sandboxed trigger execution: one fresh Lua state per firing.
//!
//! The sandbox is structural, not advisory. Each firing builds a state with
//! only pure-data stdlib (`math`, `string`, `table`, `utf8`), then installs
//! exactly three globals: `rand()` (`[0, 1)`), `say(text)` (captured, routed
//! by the caller), and the read-only `event` table (`{type}`). Lua's base
//! library ships with every state, so the entries that load code, touch the
//! host, or talk anywhere ([`STRIPPED_GLOBALS`]) are explicitly nilled — the
//! tests pin each one absent. A memory cap plus an instruction budget turn
//! runaway scripts into errors instead of hangs.

use std::cell::RefCell;
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

/// Run precompiled `bytecode` for `on`, returning every `say`ed line in order.
///
/// Any Lua failure — a runtime error, an exhausted budget — comes back as
/// `Err` text. Nothing throws across this boundary: a failing script can
/// never crash the calling system.
pub fn run_trigger(bytecode: &[u8], on: TriggerKind) -> Result<Vec<String>, String> {
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
    lua.globals()
        .set(
            "say",
            lua.create_function({
                let said = said.clone();
                move |_, text: String| {
                    said.borrow_mut().push(text);
                    Ok(())
                }
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    lua.globals()
        .set(
            "rand",
            lua.create_function(|_, ()| Ok(rand::random::<f64>()))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    let event = lua.create_table().map_err(|e| e.to_string())?;
    event.set("type", on.as_str()).map_err(|e| e.to_string())?;
    lua.globals()
        .set("event", event)
        .map_err(|e| e.to_string())?;

    lua.load(bytecode).exec().map_err(|e| e.to_string())?;
    let out = said.borrow().clone();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::compile;

    fn run(source: &str, on: TriggerKind) -> Result<Vec<String>, String> {
        run_trigger(&compile(source).expect("fixture must compile"), on)
    }

    #[test]
    fn unconditional_say_is_captured() {
        assert_eq!(
            run("say('Hello there.')", TriggerKind::Enter).unwrap(),
            ["Hello there."]
        );
    }

    #[test]
    fn chance_gate_passes_and_blocks() {
        assert_eq!(
            run("if rand() >= 0 then say('a') end", TriggerKind::Enter).unwrap(),
            ["a"]
        );
        assert!(run("if rand() < 0 then say('a') end", TriggerKind::Enter)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rand_stays_in_unit_interval() {
        let out = run(
            "for i = 1, 200 do local r = rand() if r < 0 or r >= 1 then say('OUT') end end say('done')",
            TriggerKind::Enter,
        )
        .unwrap();
        assert_eq!(out, ["done"]);
    }

    #[test]
    fn event_type_is_visible() {
        let out = run("say(event.type)", TriggerKind::AttemptLeave).unwrap();
        assert_eq!(out, ["attempt_leave"]);
    }

    #[test]
    fn multiple_says_keep_order() {
        let out = run("say('one') say('two')", TriggerKind::Leave).unwrap();
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
            "local t = 0 for _, v in pairs({1, 2}) do t = t + v end say(type('x') .. tostring(t))",
            TriggerKind::Enter,
        )
        .unwrap();
        assert_eq!(out, ["string3"]);
    }

    #[test]
    fn allowed_pure_libraries_work() {
        let out = run(
            "say(string.upper('hi') .. math.floor(1.9) .. #'abc' .. utf8.len('é'))",
            TriggerKind::Enter,
        )
        .unwrap();
        assert_eq!(out, ["HI131"], "upper + floor + len + concat");
    }
}
