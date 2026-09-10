use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use example_mud::seed;
use grim::GrimDefaultPlugins;

fn main() {
    let mut app = App::new();

    // Headless server: cap the frame loop at 60 Hz. Bare `MinimalPlugins` runs
    // `Update` with no wait, pinning a core at 99% while idle (issue #121) and
    // overflowing the change-detection window (the "has not run for N ticks"
    // WARN). Timed logic reads `Time` deltas, so the rate only sets latency.
    app.add_plugins(
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / 60.0,
        ))),
    );
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
