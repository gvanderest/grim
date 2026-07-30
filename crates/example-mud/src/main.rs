use bevy::log::LogPlugin;
use bevy::prelude::*;
use example_mud::seed;
use grim::GrimDefaultPlugins;

fn main() {
    let mut app = App::new();

    app.add_plugins(MinimalPlugins);
    app.add_plugins(LogPlugin {
        filter: "info".into(),
        ..Default::default()
    });

    // The full GRIM stack, from one facade crate. Swap `GrimDefaultPlugins` for
    // the individual plugins (all under `grim::plugins`) to omit or replace any.
    app.add_plugins(GrimDefaultPlugins { telnet_port: 4000 });

    // Seed the world
    app.add_systems(Startup, seed::seed_world);

    app.run();
}
