//! Module for statically hosting the UI assets

use axum::Router;
use memory_serve::{MemoryServe, load_assets};

pub fn ui_router() -> Router {
    MemoryServe::new(load_assets!("./ui/build"))
        .index_file(Some("/index.html"))
        .into_router()
}
