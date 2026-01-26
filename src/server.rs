use axum::{Router, routing::get};
use memory_serve::{MemoryServe, load_assets};
use tokio::{
    net::{TcpListener, ToSocketAddrs},
    sync::oneshot,
    task::JoinHandle,
};

pub struct Server {
    trigger_shutdown: oneshot::Sender<()>,
    join_handle: JoinHandle<()>,
}

impl Server {
    pub async fn new(socket_address: impl ToSocketAddrs) -> anyhow::Result<Server> {
        let listener = TcpListener::bind(socket_address).await?;
        let router = router();
        let (trigger_shutdown, shutdown_triggered) = oneshot::channel();
        let join_handle = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    shutdown_triggered
                        .await
                        .expect("Sendor for shutdown trigger must not be dropped before used.")
                })
                .await
                .expect("axum::serve must not return an error");
        });
        let server = Server {
            trigger_shutdown,
            join_handle,
        };
        Ok(server)
    }

    pub async fn shutdown(self) {
        self.trigger_shutdown.send(()).expect("Receiver must exist");
        self.join_handle.await.unwrap();
    }
}

fn router() -> Router {
    let client_ui_router = MemoryServe::new(load_assets!("./ui/build"))
        .index_file(Some("/index.html"))
        .into_router();

    Router::new()
        .merge(client_ui_router)
        .route("/health", get(|| async { "OK" }))
}
