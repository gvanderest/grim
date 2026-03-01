use bevy_ecs::prelude::*;

use std::error::Error;

mod server;
mod telnet;
mod websocket;

#[derive(Component)]
struct Character {
    name: String,
}

fn create_characters(world: &mut World) {
    world.spawn(Character {
        name: "Kelemvor".to_string(),
    });
    world.spawn(Character {
        name: "Jergal".to_string(),
    });
}

// FIXME: Debugging
fn log_players(query: Query<&Character>) {
    for character in query.iter() {
        println!("Player: {}", character.name);
    }
}

struct Game {
    // Basics
    running: bool,
    world: World,

    // Connections
    servers: Vec<Box<dyn server::Server>>,
    clients: Vec<Box<dyn server::Client>>,
}

impl Game {
    fn new() -> Self {
        let mut world = World::new();

        create_characters(&mut world);

        Self {
            clients: Vec::new(),
            running: false,
            servers: Vec::new(),
            world,
        }
    }

    async fn run(&mut self) {
        self.running = true;

        // Start servers
        for server in &self.servers {
            server.start().await;
        }

        // Log players
        let mut schedule = Schedule::default();
        schedule.add_systems(log_players);

        while self.running {
            println!("Ticking..");
            schedule.run(&mut self.world);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    async fn stop(&mut self) {
        self.running = false;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut game = Game::new();

    let telnet_server = telnet::TelnetServer::new();
    game.servers.push(Box::new(telnet_server));

    let websocket_server = websocket::WebsocketServer::new();
    game.servers.push(Box::new(websocket_server));

    game.run().await;

    // FIXME: Handle gracefully shutdown

    Ok(())
}
