use std::io::ErrorKind;
use std::net::SocketAddr;

use bytes::Bytes;
use futures_channel::mpsc::Sender;
use futures_channel::oneshot;
use futures_util::SinkExt;
use http_body::Limited;
use hyper::body::to_bytes;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, StatusCode};
use tracing::{error, info, instrument};

use crate::command::Command;
use crate::manipulate::http::response::{AddFileRequest, ListFile, ListResponse};

mod response;

const LIST_FILES_PATH: &str = "/list_files";
const ADD_FILE_PATH: &str = "/add_file";

#[derive(Debug, Clone)]
pub struct Server {
    command_sender: Sender<Command>,
}

impl Server {
    pub fn new(command_sender: Sender<Command>) -> Self {
        Self { command_sender }
    }

    pub async fn listen(self, addr: SocketAddr) -> anyhow::Result<()> {
        hyper::Server::bind(&addr)
            .serve(make_service_fn(move |_| {
                let this = self.clone();

                async move {
                    Ok::<_, hyper::Error>(service_fn(move |req| {
                        let mut this = this.clone();

                        async move { Ok::<_, hyper::Error>(this.handle(req).await) }
                    }))
                }
            }))
            .await?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn handle(&mut self, req: Request<Body>) -> Response<Body> {
        let path = req.uri().path();
        let method = req.method();
        match (path, method) {
            (LIST_FILES_PATH, &Method::GET) => self.handle_list_files(req).await,
            (ADD_FILE_PATH, &Method::POST) => self.handle_add_file(req).await,

            _ => {
                error!(path, %method, "unknown url path or method");

                Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .expect("create response failed")
            }
        }
    }

    #[instrument(skip(self))]
    async fn handle_list_files(&mut self, req: Request<Body>) -> Response<Body> {
        let include_peer = match req.uri().query() {
            None => true,
            Some(queries) => queries
                .split('&')
                .filter_map(|query| query.split_once('='))
                .any(|(key, value)| {
                    let value = value.to_lowercase();

                    key == "include_peer" && matches!(value.as_str(), "false" | "0")
                }),
        };

        self.list_files(include_peer).await
    }

    #[instrument(skip(self))]
    async fn list_files(&mut self, include_peer: bool) -> Response<Body> {
        let (sender, receiver) = oneshot::channel();

        if let Err(err) = self
            .command_sender
            .send(Command::ListFiles {
                include_peer,
                result_sender: sender,
            })
            .await
        {
            error!(%err, "send command failed");

            let response = Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .expect("create response failed");

            return response;
        }

        info!(include_peer, "send list files command done");

        let details = match receiver.await {
            Err(err) => {
                error!(%err, "receiver list files result failed");

                let response = Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .expect("create response failed");

                return response;
            }

            Ok(Err(err)) => {
                error!(%err, "list files failed");

                let response = Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .expect("create response failed");

                return response;
            }

            Ok(Ok(details)) => details,
        };

        info!(include_peer, ?details, "list files done");

        let files = details
            .into_iter()
            .map(|detail| ListFile {
                filename: detail.filename,
                hash: detail.hash,
                downloaded: detail.downloaded,
                peers: detail
                    .peers
                    .into_iter()
                    .map(|peer| peer.to_base58())
                    .collect(),
            })
            .collect();
        let list_response = ListResponse { files };

        match serde_json::to_string(&list_response) {
            Err(err) => {
                error!(%err, ?list_response, "marshal list response failed");

                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .expect("create response failed")
            }

            Ok(data) => {
                info!(?list_response, %data, "marshal list response done");

                Response::builder()
                    .header("content-type", "application/json")
                    .body(Body::from(data))
                    .expect("create response failed")
            }
        }
    }

    #[instrument(skip(self))]
    async fn handle_add_file(&mut self, req: Request<Body>) -> Response<Body> {
        let body = match read_body(req).await {
            Err(err) => return err,
            Ok(body) => body,
        };

        info!("read body done");

        let add_file_req = match serde_json::from_slice::<AddFileRequest>(&body) {
            Err(err) => {
                error!(%err, "unmarshal body failed");

                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(err.to_string()))
                    .expect("create response failed");
            }

            Ok(add_file_req) => add_file_req,
        };

        info!(?add_file_req, "unmarshal add file request done");

        let (sender, receiver) = oneshot::channel();

        let file_path = add_file_req.file_path.clone();

        if let Err(err) = self
            .command_sender
            .send(Command::AddFile {
                file_path: add_file_req.file_path.into(),
                result_sender: sender,
            })
            .await
        {
            error!(%err, "send add file command failed");

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .expect("create response failed");
        }

        match receiver.await {
            Err(err) => {
                error!(%err, "receive add file result failed");

                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .expect("create response failed")
            }

            Ok(Err(err)) if err.kind() == ErrorKind::NotFound => {
                error!(%file_path, "file not exists");

                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::empty())
                    .expect("create response failed")
            }

            Ok(Err(err)) => {
                error!(%err, %file_path, "add file failed");

                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .expect("create response failed")
            }

            Ok(Ok(_)) => {
                info!(%file_path, "add file done");

                Response::new(Body::empty())
            }
        }
    }
}

#[instrument(skip(req))]
async fn read_body(req: Request<Body>) -> Result<Bytes, Response<Body>> {
    const MAX_BODY_SIZE: usize = 16 * 1024; // 16KiB

    let body = Limited::new(req.into_body(), MAX_BODY_SIZE);
    match to_bytes(body).await {
        Err(err) => {
            error!(%err, "read body failed");

            Err(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(err.to_string()))
                .expect("create response failed"))
        }

        Ok(body) => Ok(body),
    }
}
