use bevy::log::LogPlugin;
use bevy::prelude::*;

mod seed;

fn main() {
    let mut app = App::new();

    app.add_plugins(TaskPoolPlugin::default());
    app.add_plugins(TypeRegistrationPlugin::default());
    app.add_plugins(FrameCountPlugin::default());
    app.add_plugins(TimePlugin::default());
    app.add_plugins(ScheduleRunnerPlugin::run_loop(RunMode::Loop { wait: None }));
    app.add_plugins(LogPlugin {
        filter: "info".into(),
        ..Default::default()
    });

    // Engine plugins
    app.add_plugins(grim::plugins::WorldPlugin);
    app.add_plugins(grim::plugins::SocialPlugin);
    app.add_plugins(grim::plugins::PersistencePlugin);

    // Client + Protocol
    app.add_plugins(grim_client::ClientPlugin);
    app.add_plugins(grim_protocol_telnet::TelnetPlugin::new(4000));

    // Seed the world
    app.add_systems(Startup, seed::seed_world);

    app.run();
}
