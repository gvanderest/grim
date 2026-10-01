//! The `score` command: your character sheet.
//!
//! Reads the being's `Name` + `Actor` (race/level) + `Character` (class/title/
//! xp/coin) + `Health` and answers with one `InfoMessage` block: name and
//! title, race/class/level/XP, current/max HP, coins. Missing pieces read as
//! blanks/zero (fail closed, never panic on a partial being).

use bevy::prelude::*;
use grim_core::components::Name as GrimName;
use grim_core::events::{Command, EngineCommand, InfoMessage};
use grim_text::tr;
use grim_world::{ClassRegistry, RaceRegistry};

use crate::actor::Actor;
use crate::character::Character;
use crate::combat_state::Health;

/// `score`: show your character sheet.
#[allow(clippy::too_many_arguments)] // reason: one query per sheet row; bundling hides the fields
pub(crate) fn handle_score(
    mut engine: MessageReader<EngineCommand>,
    names: Query<&GrimName>,
    actors: Query<&Actor>,
    characters: Query<&Character>,
    health: Query<&Health>,
    races: Res<RaceRegistry>,
    classes: Res<ClassRegistry>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        if !matches!(cmd.command, Command::Score) {
            continue;
        }
        let actor = cmd.client;
        let name = names.get(actor).map(|n| n.0.clone()).unwrap_or_default();
        let (race, level) = actors
            .get(actor)
            .map(|a| (a.race.clone(), a.level))
            .unwrap_or_default();
        let (class, title, xp, coin) = characters
            .get(actor)
            .map(|c| {
                (
                    c.class.clone(),
                    c.title.clone().unwrap_or_default(),
                    c.xp,
                    c.coin,
                )
            })
            .unwrap_or_default();
        let (hp, max_hp) = health
            .get(actor)
            .map(|h| (h.current, h.max))
            .unwrap_or((0, 0));
        let race_name = races
            .get(&race)
            .map(|r| r.name.clone())
            .unwrap_or_else(|| race.clone());
        let class_name = classes
            .get(&class)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| class.clone());
        let xp_text = xp.to_string();
        let level_text = level.to_string();
        let hp_text = hp.to_string();
        let max_hp_text = max_hp.to_string();
        let coin_text = coin.to_string();
        info.write(InfoMessage {
            target: actor,
            text: tr!(
                "score.sheet",
                name = name.as_str(),
                title = title.as_str(),
                folk = race_name.as_str(),
                vocation = class_name.as_str(),
                level = level_text.as_str(),
                tally = xp_text.as_str(),
                hp = hp_text.as_str(),
                vigor = max_hp_text.as_str(),
                purse = coin_text.as_str()
            ),
        });
    }
}

/// Wire the `score` handler and the messages it reads/emits.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .init_resource::<RaceRegistry>()
        .init_resource::<ClassRegistry>()
        .add_systems(Update, handle_score);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Creature;
    use crate::placement::InRoom;
    use grim_core::GrimId;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        register(&mut app);
        app
    }

    fn spawn_being(app: &mut App) -> Entity {
        let room = app.world_mut().spawn_empty().id();
        app.world_mut()
            .spawn((
                GrimName("Hero".into()),
                Actor {
                    race: "human".into(),
                    level: 3,
                    gender: grim_core::character::Gender::Male,
                },
                Character {
                    id: GrimId::new(),
                    account_id: GrimId::new(),
                    created_at: chrono::Utc::now(),
                    last_room: None,
                    roles: Vec::new(),
                    class: "warrior".into(),
                    title: Some("the Brave".into()),
                    restrings: Default::default(),
                    config: Default::default(),
                    xp: 120,
                    coin: 45,
                },
                Health::full(100),
                InRoom { room },
            ))
            .id()
    }

    fn send_score(app: &mut App, actor: Entity) {
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Score,
        });
    }

    fn infos(app: &App) -> Vec<String> {
        let messages = app.world().resource::<Messages<InfoMessage>>();
        messages
            .get_cursor()
            .read(messages)
            .map(|m| m.text.clone())
            .collect()
    }

    #[test]
    fn score_lists_sheet_fields() {
        let mut app = test_app();
        let hero = spawn_being(&mut app);
        send_score(&mut app, hero);
        app.update();
        let out = infos(&app).join("");
        assert!(out.contains("Hero"), "name; got:\n{out}");
        assert!(out.contains("the Brave"), "title; got:\n{out}");
        assert!(out.contains("Human"), "race; got:\n{out}");
        assert!(out.contains("Warrior"), "class; got:\n{out}");
        assert!(out.contains('3'), "level; got:\n{out}");
        assert!(out.contains("120"), "xp; got:\n{out}");
        assert!(out.contains("100"), "hp; got:\n{out}");
        assert!(out.contains("45"), "coin; got:\n{out}");
    }

    #[test]
    fn score_without_health_reads_zero() {
        let mut app = test_app();
        let room = app.world_mut().spawn_empty().id();
        let bare = app
            .world_mut()
            .spawn((GrimName("Bare".into()), InRoom { room }))
            .id();
        send_score(&mut app, bare);
        app.update();
        let out = infos(&app).join("");
        assert!(out.contains("Bare"), "name still renders; got:\n{out}");
        assert!(out.contains('0'), "missing pools read zero; got:\n{out}");
    }

    #[test]
    fn score_ignores_other_commands() {
        let mut app = test_app();
        let hero = spawn_being(&mut app);
        app.world_mut().write_message(EngineCommand {
            client: hero,
            command: Command::Quit,
        });
        app.update();
        assert!(infos(&app).is_empty());
    }

    #[test]
    fn creature_without_character_scores_blank_class() {
        let mut app = test_app();
        let room = app.world_mut().spawn_empty().id();
        let mob = app
            .world_mut()
            .spawn((
                GrimName("Wolf".into()),
                Actor {
                    race: String::new(),
                    level: 1,
                    gender: grim_core::character::Gender::Neutral,
                },
                Creature,
                Health::full(20),
                InRoom { room },
            ))
            .id();
        send_score(&mut app, mob);
        app.update();
        let out = infos(&app).join("");
        assert!(out.contains("Wolf"), "mob name renders; got:\n{out}");
    }
}
