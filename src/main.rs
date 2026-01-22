use axum::{Router, routing::get};
use dotenv::dotenv;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load '.env' file; Ignore errors.
    dotenv().ok();

    let endpoint = std::env::var("ENDPOINT").unwrap_or_else(|_| "0.0.0.0:3000".to_string());

    let app = Router::new().route("/", get(|| async { "Hello, World!" }));
    let listener = TcpListener::bind(endpoint).await?;
    axum::serve(listener, app)
        .await
        .expect("axum::serve never completes");
    Ok(())
}
