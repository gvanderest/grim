//! SPIKE (throwaway): prove the boxed command-handler shape for ARCHITECTURE.md §5.2/§7.
//!
//! Question: can `App::add_command::<E: Event>(name, factory)` erase a concrete
//! event type `E` into a single boxed handler type stored in a registry, such
//! that invoking the handler triggers `E` and a plain `add_observer` fires?
//!
//! The lifetime worry: `Commands<'w, 's>` is lifetime-parametric, so the boxed
//! `Fn` must carry a higher-ranked bound `for<'w, 's>`.
//!
//! Run: cargo run -p grim --example spike_handler

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;

/// The erased handler. HRTB over both Commands lifetimes is the crux.
type Handler = Box<dyn for<'w, 's> Fn(&mut Commands<'w, 's>, Entity, &str) + Send + Sync>;

#[derive(Resource, Default)]
struct CommandRegistry {
    handlers: HashMap<String, Handler>,
}

/// Extension trait mirroring the proposed `App::add_command`.
trait AddCommand {
    fn add_command<E, F>(&mut self, name: &str, factory: F) -> &mut Self
    where
        E: Event,
        F: Fn(Entity, &str) -> E + Send + Sync + 'static,
        for<'a> E::Trigger<'a>: Default;
}

impl AddCommand for App {
    fn add_command<E, F>(&mut self, name: &str, factory: F) -> &mut Self
    where
        E: Event,
        F: Fn(Entity, &str) -> E + Send + Sync + 'static,
        for<'a> E::Trigger<'a>: Default,
    {
        // Erase E: the boxed handler closes over `factory` and triggers the
        // concrete event. Nothing outside this fn ever names E again.
        let handler: Handler = Box::new(move |commands, actor, rest| {
            commands.trigger(factory(actor, rest));
        });
        let mut reg = self
            .world_mut()
            .get_resource_or_insert_with(CommandRegistry::default);
        reg.handlers.insert(name.to_string(), handler);
        self
    }
}

// --- two unrelated concrete events, as two "plugins" would define ---

#[derive(Event)]
struct Look {
    actor: Entity,
    target: Option<String>,
}

#[derive(Event)]
struct Kill {
    actor: Entity,
    victim: String,
}

// Proof of dispatch: observers write to a shared log.
type Log = Arc<Mutex<Vec<String>>>;

fn main() {
    let log: Log = Arc::new(Mutex::new(Vec::new()));

    let mut app = App::new();

    // register two commands from "different plugins"
    app.add_command("look", |actor, rest| Look {
        actor,
        target: (!rest.is_empty()).then(|| rest.to_string()),
    });
    app.add_command("kill", |actor, rest| Kill {
        actor,
        victim: rest.to_string(),
    });

    // observers, one per event type — keyed on the event type, not broadcast
    let l = log.clone();
    app.world_mut()
        .add_observer(move |ev: On<Look>, _c: Commands| {
            l.lock().unwrap().push(format!(
                "on_look actor={:?} target={:?}",
                ev.actor, ev.target
            ));
        });
    let l = log.clone();
    app.world_mut()
        .add_observer(move |ev: On<Kill>, _c: Commands| {
            l.lock()
                .unwrap()
                .push(format!("on_kill actor={:?} victim={}", ev.actor, ev.victim));
        });

    let actor = app.world_mut().spawn_empty().id();

    // simulate the router: resolve name -> boxed handler -> run against Commands.
    for (name, rest) in [("look", "north"), ("kill", "goblin"), ("look", "")] {
        let world = app.world_mut();
        world.resource_scope(|world, reg: Mut<CommandRegistry>| {
            let Some(handler) = reg.handlers.get(name) else {
                return;
            };
            let mut queue = bevy::ecs::world::CommandQueue::default();
            let mut commands = Commands::new(&mut queue, world);
            handler(&mut commands, actor, rest);
            queue.apply(world);
        });
    }

    let out = log.lock().unwrap().clone();
    println!("--- observer log ---");
    for line in &out {
        println!("{line}");
    }
    assert_eq!(out.len(), 3, "expected 3 observer fires, got {}", out.len());
    println!("SPIKE OK: HRTB boxed handler + typed trigger + add_observer works.");
}
