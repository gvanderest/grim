//! Scene stack, thin slice (ADR-0003 Option B).
//!
//! A session's [`SceneStack`] is the ordered set of scenes it occupies; input
//! is interpreted by the topmost scene. Each scene is an entity carrying its
//! own marker component (today only [`InGameScene`]), spawned as a **child**
//! of the session entity so a disconnect despawn cascades — no explicit pop
//! needed on the way out.
//!
//! Interim scope: only the in-game scene exists. `ClientState` still drives
//! every pre-game prompt in the auth crate; the stack mirrors *only* world
//! entry (pushed once per session at the transition — see the `JustEnteredWorld`
//! guard in the auth input dispatcher and `finalize_resume` here). Routing
//! reads the stack top, never `ClientState`. Multi-scene push/pop (editor over
//! game, per-scene data, output policy) lands with the rest of ADR-0003.
//!
//! Invariant: at most one push per session entity. A session enters the world
//! exactly once per lifetime (a reconnect is a new session entity), so the
//! overwrite-`insert` in [`push_ingame_scene`] is safe until real stack ops
//! arrive.

use bevy::prelude::*;

/// Marker: the session is standing in the world, dispatching command lines.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct InGameScene;

/// Ordered scene stack on the session entity. Top of stack is the last id.
#[derive(Component, Debug, Default)]
pub struct SceneStack(pub Vec<Entity>);

/// Push the in-game scene onto `session`: spawn the marker as a child (so it
/// dies with the session) and record it as the stack top.
pub fn push_ingame_scene(commands: &mut Commands, session: Entity) -> Entity {
    let scene = commands.spawn(InGameScene).id();
    commands
        .entity(session)
        .add_child(scene)
        .insert(SceneStack(vec![scene]));
    scene
}

/// True when the stack top satisfies `has` (the caller checks the scene
/// marker — a query in systems, a world lookup in tests). Missing/empty stacks
/// read as *not* in game — fail closed, never fall through to dispatch.
pub fn top_is_ingame(stack: &SceneStack, has: impl Fn(Entity) -> bool) -> bool {
    stack.0.last().is_some_and(|top| has(*top))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_records_top_as_child() {
        let mut app = App::new();
        let session = app.world_mut().spawn_empty().id();
        let scene = {
            let mut commands = app.world_mut().commands();
            push_ingame_scene(&mut commands, session)
        };
        app.world_mut().flush();

        let stack = app.world().get::<SceneStack>(session).unwrap();
        assert_eq!(stack.0, vec![scene]);
        let children = app.world().get::<Children>(session).unwrap();
        assert_eq!(children.iter().collect::<Vec<_>>(), [scene]);
    }

    #[test]
    fn top_is_ingame_fails_closed() {
        let mut app = App::new();
        // Empty stack: not in game.
        let session = app.world_mut().spawn(SceneStack::default()).id();
        // Unrelated scene entity on top: not in game.
        let other = app.world_mut().spawn_empty().id();
        let ingame = app.world_mut().spawn(InGameScene).id();

        let stack = app.world().get::<SceneStack>(session).unwrap();
        assert!(!top_is_ingame(stack, |e| app
            .world()
            .get::<InGameScene>(e)
            .is_some()));

        app.world_mut()
            .entity_mut(session)
            .insert(SceneStack(vec![other]));
        let stack = app.world().get::<SceneStack>(session).unwrap();
        assert!(!top_is_ingame(stack, |e| app
            .world()
            .get::<InGameScene>(e)
            .is_some()));

        app.world_mut()
            .entity_mut(session)
            .insert(SceneStack(vec![ingame]));
        let stack = app.world().get::<SceneStack>(session).unwrap();
        assert!(top_is_ingame(stack, |e| app
            .world()
            .get::<InGameScene>(e)
            .is_some()));
    }

    #[test]
    fn stack_dies_with_session() {
        let mut app = App::new();
        let session = app.world_mut().spawn_empty().id();
        let scene = {
            let mut commands = app.world_mut().commands();
            push_ingame_scene(&mut commands, session)
        };
        app.world_mut().flush();

        app.world_mut().despawn(session);
        app.world_mut().flush();
        assert!(app.world().get_entity(scene).is_err());
    }
}
