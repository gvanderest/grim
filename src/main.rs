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

    // Client
    app.add_plugins(grim_client::ClientPlugin);

    // Protocol
    app.add_plugins(grim_protocol_telnet::TelnetPlugin::new(4000));

    // Seed the world
    app.add_systems(Startup, seed::seed_world);

    app.run();
}