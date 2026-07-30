use bevy::log::LogPlugin;
use bevy::prelude::*;

mod seed;

fn main() {
    let mut app = App::new();

    app.add_plugins(MinimalPlugins);
    app.add_plugins(LogPlugin {
        filter: "info".into(),
        ..Default::default()
    });

    // Engine plugins
    app.add_plugins(grim::plugins::WorldPlugin);
    app.add_plugins(grim::plugins::SocialPlugin);
    app.add_plugins(grim::plugins::PersistencePlugin);
    app.add_plugins(grim::plugins::ShutdownPlugin);

    // Client + Protocol
    app.add_plugins(grim_scene::ScenePlugin);
    app.add_plugins(grim_networking_telnet::TelnetPlugin::new(4000));

    // Seed the world
    app.add_systems(Startup, seed::seed_world);

    app.run();
}
