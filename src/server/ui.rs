//! Module for statically hosting the UI assets

use axum::Router;
use static_serve::embed_assets;

// `strip_html_ext` stirps `.html` extension from `index.html` and serves it as root `/` path.
embed_assets!("ui/build", strip_html_ext = true);

pub fn ui_router() -> Router {
    // static_router method is created by embed_assets macro.
    static_router()
}
