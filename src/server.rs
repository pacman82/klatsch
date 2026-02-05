mod http_api;
mod last_event_id;
mod terminate_on_shutdown;
mod ui;

use axum::{Router, routing::get};

use tokio::{
    net::{TcpListener, ToSocketAddrs},
    sync::{oneshot, watch},
    task::JoinHandle,
};

use crate::chat::Chat;

use self::{http_api::api_router, ui::ui_router};

pub struct Server {
    /// Indicates whether the server is about to shut down. Long-lived requests like event streams
    /// watch this in order to short circut and allow the the graceful shutdown to complete faster.
    shutting_down: watch::Sender<bool>,
    trigger_shutdown: oneshot::Sender<()>,
    join_handle: JoinHandle<()>,
}

impl Server {
    pub async fn new<C>(socket_address: impl ToSocketAddrs, chat: C) -> anyhow::Result<Server>
    where
        C: Chat + Send + Sync + Clone + 'static,
    {
        let listener = TcpListener::bind(socket_address).await?;
        let (shutting_down_sender, shutting_down_receiver) = watch::channel(false);
        let router = router(chat, shutting_down_receiver);
        let (trigger_shutdown, shutdown_triggered) = oneshot::channel();
        let join_handle = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    shutdown_triggered
                        .await
                        .expect("Sender for shutdown trigger must not be dropped before used.")
                })
                .await
                .expect("axum::serve must not return an error");
        });
        let server = Server {
            shutting_down: shutting_down_sender,
            trigger_shutdown,
            join_handle,
        };
        Ok(server)
    }

    pub async fn shutdown(self) {
        self.shutting_down.send(true).expect("Receiver must exist");
        self.trigger_shutdown.send(()).expect("Receiver must exist");
        self.join_handle.await.unwrap();
    }
}

fn router<C>(chat: C, shutting_down: watch::Receiver<bool>) -> Router
where
    C: Chat + Send + Sync + Clone + 'static,
{
    Router::new()
        .route("/health", get(|| async { "OK" }))
        .merge(api_router(chat, shutting_down))
        .merge(ui_router())
}
