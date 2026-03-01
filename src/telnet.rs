use crate::server;

pub struct TelnetServer {}

impl TelnetServer {
    pub fn new() -> Self {
        println!("Creating telnet server..");
        Self {}
    }
}

#[async_trait::async_trait]
impl server::Server for TelnetServer {
    async fn start(&self) {
        println!("Starting telnet server..")
    }
    fn get_name(&self) -> String {
        String::from("TelnetServer")
    }
}
