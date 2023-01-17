use std::future::{ready, Ready};
use std::io::Error;
use std::path::Path;

use axum::body::HttpBody;
use axum::routing::get_service;
use axum::Router;
use http::StatusCode;
use tower_http::compression::predicate::NotForContentType;
use tower_http::compression::{CompressionLayer, DefaultPredicate, Predicate};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Debug)]
pub struct StaticRouter<'a> {
    serve_assets_at: &'a str,
    dir: &'a Path,
}

impl<'a> StaticRouter<'a> {
    pub fn new<P1: AsRef<str> + ?Sized, P2: AsRef<Path> + ?Sized>(
        serve_assets_at: &'a P1,
        dir: &'a P2,
    ) -> Self {
        Self {
            serve_assets_at: serve_assets_at.as_ref(),
            dir: dir.as_ref(),
        }
    }
}

impl<'a, S, B> From<StaticRouter<'a>> for Router<S, B>
where
    B: HttpBody + Send + 'static,
    S: Clone + Send + Sync + 'static,
{
    fn from(static_router: StaticRouter) -> Self {
        let assets_service = get_service(ServeDir::new(static_router.dir).precompressed_br())
            .handle_error(handle_error);

        Router::new()
            .nest_service(static_router.serve_assets_at, assets_service)
            .fallback_service(
                get_service(
                    ServeFile::new(static_router.dir.join("index.html")).precompressed_br(),
                )
                .handle_error(handle_error),
            )
            .layer(CompressionLayer::new().br(true).compress_when(
                DefaultPredicate::new().and(NotForContentType::const_new("application/font-sfnt")),
            ))
    }
}

fn handle_error(_: Error) -> Ready<StatusCode> {
    ready(StatusCode::INTERNAL_SERVER_ERROR)
}
