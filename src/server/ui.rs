//! Module for statically hosting the UI assets

use axum::Router;

pub fn ui_router() -> Router {
    memory_serve::load!()
        .index_file(Some("/index.html"))
        .into_router()
}
