pub trait Client {}

#[async_trait::async_trait]
pub trait Server {
    async fn start(&self);
    fn get_name(&self) -> String;
}
