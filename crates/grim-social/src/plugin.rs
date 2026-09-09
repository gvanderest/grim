//! `SocialPlugin`: load the registry at startup, register each social name
//! into the shared [`CommandRegistry`], and run the handler.

use bevy::prelude::*;
use grim_command::CommandRegistry;
use grim_core::events::{Command, EngineCommand, InfoMessage};

use crate::handler::{handle_social, SocialPerformed};
use crate::social::{load_socials, SocialDir};

/// Data-driven socials (`grin`, `smile`, …). Add after `ScenePlugin` (which
/// inserts the [`CommandRegistry`] resource) so names register into it.
pub struct SocialPlugin;

impl Plugin for SocialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SocialDir>();
        app.add_message::<EngineCommand>()
            .add_message::<InfoMessage>()
            .add_message::<SocialPerformed>()
            .add_systems(Startup, load_and_register)
            .add_systems(Update, handle_social);
    }
}

/// Load built-ins + `data/socials/*.json`, then register every name as a
/// command producing [`Command::Social`]. Registration runs after the static
/// commands, so each social is [`CommandRegistry::deprioritize`]d back down —
/// statics keep their prefixes. An exact static collision skips the social
/// with a warn (statics win; fail closed, no shadow).
fn load_and_register(
    dir: Res<SocialDir>,
    mut commands: Commands,
    registry: Option<ResMut<CommandRegistry<Command>>>,
) {
    let socials = load_socials(&dir.0);
    if let Some(mut reg) = registry {
        let mut names: Vec<String> = socials.names().map(str::to_string).collect();
        names.sort();
        for name in &names {
            if reg.contains(name) {
                warn!(
                    "social '{name}' collides with a static command; static wins, social skipped"
                );
                continue;
            }
            let owned = name.clone();
            reg.register(name, move |rest| {
                Some(Command::Social {
                    name: owned.clone(),
                    target: rest.split_whitespace().next().map(str::to_string),
                })
            });
            reg.deprioritize(name);
            reg.set_section(name, "social");
        }
        // The `socials` lister itself stays in the default section (so
        // `commands` advertises it) but deprioritized like the verbs.
        if !reg.contains("socials") {
            reg.register("socials", |_| Some(Command::SocialList));
            reg.deprioritize("socials");
        }
        for contest in reg.contested_prefixes() {
            let involves_social = names
                .iter()
                .any(|n| *n == contest.winner || contest.shadowed.iter().any(|s| s == n));
            if involves_social {
                warn!(
                    "command prefix '{}' resolves to '{}', shadowing {}",
                    contest.prefix,
                    contest.winner,
                    contest.shadowed.join(", ")
                );
            }
        }
    }
    commands.insert_resource(socials);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::SocialRegistry;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(CommandRegistry::<Command>::new());
        app.add_plugins(SocialPlugin);
        app
    }

    fn fixture_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("grim-social-plugin-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn plugin_registers_builtins_after_statics() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<CommandRegistry<Command>>()
            .register("say", |rest| {
                (!rest.is_empty()).then(|| Command::Say {
                    text: rest.to_string(),
                })
            });
        app.world_mut()
            .insert_resource(SocialDir("no-such-dir".into()));
        app.update();
        let reg = app.world().resource::<CommandRegistry<Command>>();
        // Built-ins resolve, solo and targeted.
        assert_eq!(
            reg.resolve("grin", ""),
            Some(Command::Social {
                name: "grin".into(),
                target: None
            })
        );
        assert_eq!(
            reg.resolve("grin", "bob"),
            Some(Command::Social {
                name: "grin".into(),
                target: Some("bob".into())
            })
        );
        // Deprioritized: the static keeps its prefix.
        assert!(matches!(reg.resolve("s", "hi"), Some(Command::Say { .. })));
        // Socials are section-tagged; the `socials` lister resolves too.
        assert!(reg.names_in_section("social").contains(&"grin".to_string()));
        assert_eq!(reg.resolve("socials", ""), Some(Command::SocialList));
        // Registry resource is populated for the handler.
        assert!(app
            .world()
            .resource::<SocialRegistry>()
            .get("smile")
            .is_some());
    }

    #[test]
    fn exact_static_collision_skips_the_social() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<CommandRegistry<Command>>()
            .register("grin", |_| Some(Command::Who));
        app.world_mut()
            .insert_resource(SocialDir("no-such-dir".into()));
        app.update();
        let reg = app.world().resource::<CommandRegistry<Command>>();
        assert_eq!(reg.resolve("grin", ""), Some(Command::Who));
    }

    #[test]
    fn file_verb_registers_from_disk() {
        let dir = fixture_dir("disk");
        std::fs::write(
            dir.join("smirk.json"),
            r#"{"solo": {"actor": "You smirk.\n"}}"#,
        )
        .unwrap();
        let mut app = test_app();
        app.world_mut().insert_resource(SocialDir(dir.clone()));
        app.update();
        let reg = app.world().resource::<CommandRegistry<Command>>();
        assert!(matches!(
            reg.resolve("smirk", ""),
            Some(Command::Social { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_command_registry_is_not_fatal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(SocialPlugin);
        app.update();
        assert!(app
            .world()
            .resource::<SocialRegistry>()
            .get("grin")
            .is_some());
    }
}
