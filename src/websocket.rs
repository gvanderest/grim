use crate::server;

pub struct WebsocketServer {}

impl WebsocketServer {
    pub fn new() -> Self {
        println!("Creating websocket server..");
        Self {}
    }
}

#[async_trait::async_trait]
impl server::Server for WebsocketServer {
    async fn start(&self) {
        println!("Starting websocket server..")
    }
    fn get_name(&self) -> String {
        String::from("WebsocketServer")
    }
}
