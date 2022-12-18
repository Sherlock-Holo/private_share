#[tokio::main]
async fn main() -> anyhow::Result<()> {
    private_share::run().await
}
