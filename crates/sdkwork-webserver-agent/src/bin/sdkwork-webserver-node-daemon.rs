#[tokio::main]
async fn main() -> anyhow::Result<()> {
    sdkwork_webserver_agent::run().await
}
