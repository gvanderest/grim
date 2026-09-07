//! `ScriptPlugin`: run sandboxed mob triggers on room transitions.
//!
//! Layers on `grim-actor` (which owns and fires the transition events) and
//! `grim-channel` (whose `say` channel carries mob speech). Compose after both.

use bevy::prelude::*;

use crate::watch::{on_attempt_enter, on_attempt_leave, on_attempt_walk, on_enter, on_leave};

/// Sandboxed Lua triggers on room entry/exit. See the crate docs for the
/// sandbox contract.
pub struct ScriptPlugin;

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_attempt_walk)
            .add_observer(on_attempt_leave)
            .add_observer(on_attempt_enter)
            .add_observer(on_leave)
            .add_observer(on_enter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim_actor::{Creature, Enter, InRoom};
    use grim_channel::ChannelMessage;
    use grim_core::components::Name as GrimName;

    use crate::trigger::{compile, ScriptTriggers, TriggerKind};

    /// The plugin composes with the channel plugin it layers on: a transition
    /// reaches a scripted mob and its speech leaves on the default `say`
    /// channel with no manual registry setup.
    #[test]
    fn script_plugin_composes_with_channel_plugin() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(grim_channel::ChannelPlugin)
            .add_plugins(ScriptPlugin);
        let room = app.world_mut().spawn_empty().id();
        let mover = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            Creature,
            GrimName("Grimmok".into()),
            InRoom { room },
            ScriptTriggers(vec![crate::trigger::CompiledTrigger {
                on: TriggerKind::Enter,
                bytecode: compile("say('yo')").unwrap(),
            }]),
        ));
        app.world_mut().trigger(Enter { actor: mover, room });
        app.update();

        let messages = app.world().resource::<Messages<ChannelMessage>>();
        let mut cursor = messages.get_cursor();
        let mut iter = cursor.read(messages);
        let ev = iter.next().expect("mob speech on the say channel");
        assert_eq!(ev.channel.name, "say");
        assert_eq!(ev.text, "yo");
        assert!(iter.next().is_none(), "exactly one speech event");
    }
}
