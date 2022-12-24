use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart};
use axum::routing::{get, post};
use axum::Router;
use byte_unit::Byte;
use bytes::{BufMut, Bytes, BytesMut};
use futures_channel::mpsc::{Receiver, Sender};
use futures_channel::{mpsc, oneshot};
use futures_util::{SinkExt, StreamExt};
use http::{Request, Response, StatusCode};
use http_body::{Body as _, Limited};
use tracing::{error, info, instrument};

use crate::command::Command;
use crate::manipulate::http::response::{AddFileRequest, ListFile, ListResponse};

mod response;

const LIST_FILES_PATH: &str = "/list_files";
const ADD_FILE_PATH: &str = "/add_file";
const UPLOAD_FILE_PATH: &str = "/upload_file";

#[derive(Debug, Clone)]
pub struct Server {
    command_sender: Sender<Command<Receiver<io::Result<Bytes>>>>,
}

impl Server {
    pub fn new(command_sender: Sender<Command<Receiver<io::Result<Bytes>>>>) -> Self {
        Self { command_sender }
    }

    pub async fn listen(self, addr: SocketAddr) -> anyhow::Result<()> {
        let router = Router::new()
            .route(
                LIST_FILES_PATH,
                get({
                    let mut this = self.clone();

                    move |body| async move { this.handle_list_files(body).await }
                }),
            )
            .route(
                ADD_FILE_PATH,
                post({
                    let mut this = self.clone();

                    move |body| async move { this.handle_add_file(body).await }
                }),
            )
            .route(
                UPLOAD_FILE_PATH,
                post({
                    let mut this = self.clone();

                    move |body| async move { this.handle_upload_file(body).await }
                }),
            )
            .layer(DefaultBodyLimit::disable());

        axum::Server::bind(&addr)
            .serve(router.into_make_service())
            .await?;

        Ok(())
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
                size: Byte::from_bytes(detail.size)
                    .get_appropriate_unit(true)
                    .to_string(),
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

    #[instrument(skip(self))]
    async fn handle_upload_file(&mut self, mut req: Multipart) -> Response<Body> {
        let mut field = match req.next_field().await {
            Err(err) => {
                error!(%err, "get next field failed");

                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(err.to_string()))
                    .expect("create response failed");
            }

            Ok(None) => {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from("empty content"))
                    .expect("create response failed");
            }

            Ok(Some(field)) => field,
        };

        let filename = match field.file_name() {
            None => {
                error!("no filename found");

                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from("no filename found"))
                    .expect("create response failed");
            }

            Some(filename) => filename.to_string(),
        };

        let (result_sender, result_receiver) = oneshot::channel();
        let (mut file_sender, file_stream) = mpsc::channel(1);

        if let Err(err) = self
            .command_sender
            .send(Command::UploadFile {
                filename: filename.to_string(),
                hash: None,
                file_stream,
                result_sender,
            })
            .await
        {
            error!(%err, %filename, "send upload file command failed");

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .expect("create response failed");
        }

        while let Some(result) = field.next().await {
            match result {
                Err(err) => {
                    error!(?err, %filename, "read upload file data failed");

                    let resp = Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from(err.to_string()))
                        .expect("create response failed");

                    let _ = file_sender
                        .send(Err(io::Error::new(ErrorKind::Other, err)))
                        .await;

                    return resp;
                }

                Ok(data) => {
                    if let Err(err) = file_sender.send(Ok(data)).await {
                        error!(%err, %filename, "send upload file data failed");

                        return Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::from(err.to_string()))
                            .expect("create response failed");
                    }

                    info!(%filename, "send upload file data done");
                }
            }
        }

        info!(%filename, "read all upload file data done");

        if let Err(err) = file_sender.flush().await {
            error!(%err, %filename, "flush file sender failed");

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(err.to_string()))
                .expect("create response failed");
        }

        drop(file_sender);

        match result_receiver.await {
            Err(err) => {
                error!(%err, %filename, "receive result failed");

                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(err.to_string()))
                    .expect("create response failed")
            }

            Ok(Err(err)) => {
                error!(%err, %filename, "handle upload file command failed");

                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(err.to_string()))
                    .expect("create response failed")
            }

            Ok(Ok(_)) => {
                info!(%filename, "upload file done");

                Response::new(Body::empty())
            }
        }
    }
}

#[instrument(skip(req))]
async fn read_body(req: Request<Body>) -> Result<Bytes, Response<Body>> {
    const MAX_BODY_SIZE: usize = 16 * 1024; // 16KiB

    let mut buf = BytesMut::with_capacity(MAX_BODY_SIZE);
    let mut body = Limited::new(req.into_body(), MAX_BODY_SIZE);
    while let Some(data) = body.data().await {
        match data {
            Err(err) => {
                error!(%err, "read body failed");

                return Err(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(err.to_string()))
                    .expect("create response failed"));
            }

            Ok(data) => {
                buf.put(data);
            }
        }
    }

    Ok(buf.freeze())
}
