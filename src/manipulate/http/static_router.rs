use std::future::{ready, Ready};
use std::io::Error;

use axum::body::HttpBody;
use axum::routing::get_service;
use axum::Router;
use http::StatusCode;
use http_dir::fs::include_dir::IncludeDirFilesystem;
use http_dir::ServeDir;

use super::static_resources::WEB_RESOURCES_DIR;

#[derive(Debug, Default)]
pub struct StaticRouter;

impl<S, B> From<StaticRouter> for Router<S, B>
where
    B: HttpBody + Send + 'static,
    S: Clone + Send + Sync + 'static,
{
    fn from(_: StaticRouter) -> Self {
        let filesystem = IncludeDirFilesystem::new(WEB_RESOURCES_DIR.clone());

        let assets_service =
            get_service(ServeDir::new(filesystem).precompressed_br()).handle_error(handle_error);

        Router::new().nest_service("/", assets_service)
    }
}

fn handle_error(_: Error) -> Ready<StatusCode> {
    ready(StatusCode::INTERNAL_SERVER_ERROR)
}
