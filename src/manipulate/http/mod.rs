use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;

use axum::extract::{DefaultBodyLimit, Multipart, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::routing::SpaRouter;
use byte_unit::Byte;
use bytes::Bytes;
use futures_channel::mpsc::Sender;
use futures_channel::{mpsc, oneshot};
use futures_util::{SinkExt, Stream, StreamExt, TryStreamExt};
use http::StatusCode;
use tracing::{error, info, instrument};

use crate::command::Command;
use crate::manipulate::http::response::{
    AddFileRequest, ListFile, ListFilesQuery, ListPeer, ListPeersResponse, ListResponse,
};

mod response;

const LIST_FILES_PATH: &str = "/list_files";
const ADD_FILE_PATH: &str = "/add_file";
const UPLOAD_FILE_PATH: &str = "/upload_file";
const LIST_PEERS_PATH: &str = "/list_peers";

type UploadFileReceiver = impl Stream<Item = io::Result<Bytes>> + Unpin + Send + 'static;

#[derive(Debug, Clone)]
pub struct Server {
    command_sender: Sender<Command<UploadFileReceiver>>,
}

impl Server {
    pub fn new(command_sender: Sender<Command<UploadFileReceiver>>) -> Self {
        Self { command_sender }
    }

    pub async fn listen(self, addr: SocketAddr, http_ui_resources: &str) -> anyhow::Result<()> {
        let api_router =
            Router::new()
                .route(
                    LIST_FILES_PATH,
                    get(|State(mut server): State<Server>, body| async move {
                        server.handle_list_files(body).await
                    }),
                )
                .merge(Router::new().route(
                    ADD_FILE_PATH,
                    post(|State(mut server): State<Server>, body| async move {
                        server.handle_add_file(body).await
                    }),
                ))
                .route(
                    UPLOAD_FILE_PATH,
                    post(|State(mut server): State<Server>, body| async move {
                        server.handle_upload_file(body).await
                    }),
                )
                .route(
                    LIST_PEERS_PATH,
                    get(|State(mut server): State<Server>| async move {
                        server.handle_list_peers().await
                    }),
                )
                .layer(DefaultBodyLimit::disable());

        let router = Router::new()
            .nest("/api", api_router)
            .merge(SpaRouter::new("/ui", http_ui_resources))
            .with_state(self);

        axum::Server::bind(&addr)
            .serve(router.into_make_service())
            .await?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn handle_list_files(
        &mut self,
        Query(query): Query<ListFilesQuery>,
    ) -> Result<Json<ListResponse>, StatusCode> {
        self.list_files(query.include_peer.unwrap_or(true)).await
    }

    #[instrument(skip(self))]
    async fn list_files(&mut self, include_peer: bool) -> Result<Json<ListResponse>, StatusCode> {
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

            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        info!(include_peer, "send list files command done");

        let details = match receiver.await {
            Err(err) => {
                error!(%err, "receiver list files result failed");

                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }

            Ok(Err(err)) => {
                error!(%err, "list files failed");

                return Err(StatusCode::INTERNAL_SERVER_ERROR);
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

        Ok(Json(list_response))
    }

    #[instrument(skip(self))]
    async fn handle_add_file(
        &mut self,
        Json(req): Json<AddFileRequest>,
    ) -> Result<(), (StatusCode, String)> {
        let (sender, receiver) = oneshot::channel();

        let file_path = req.file_path.clone();

        if let Err(err) = self
            .command_sender
            .send(Command::AddFile {
                file_path: req.file_path.into(),
                result_sender: sender,
            })
            .await
        {
            error!(%err, "send add file command failed");

            return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
        }

        match receiver.await {
            Err(err) => {
                error!(%err, "receive add file result failed");

                Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }

            Ok(Err(err)) if err.kind() == ErrorKind::NotFound => {
                error!(%file_path, "file not exists");

                Err((StatusCode::NOT_FOUND, String::new()))
            }

            Ok(Err(err)) => {
                error!(%err, %file_path, "add file failed");

                Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }

            Ok(Ok(_)) => {
                info!(%file_path, "add file done");

                Ok(())
            }
        }
    }

    #[instrument(skip(self))]
    async fn handle_upload_file(&mut self, mut req: Multipart) -> Result<(), (StatusCode, String)> {
        let field = match req.next_field().await {
            Err(err) => {
                error!(%err, "get next field failed");

                return Err((StatusCode::BAD_REQUEST, err.to_string()));
            }

            Ok(None) => return Err((StatusCode::BAD_REQUEST, "empty content".to_string())),

            Ok(Some(field)) => field,
        };

        let filename = match field.file_name() {
            None => {
                error!("no filename found");

                return Err((StatusCode::BAD_REQUEST, "no filename found".to_string()));
            }

            Some(filename) => filename.to_string(),
        };

        let (result_sender, result_receiver) = oneshot::channel();
        let (file_sender, file_stream) = mpsc::channel(1);
        let mut file_sender = file_sender.sink_map_err(|err| {
            error!(%err, %filename, "send upload file data failed");

            io::Error::new(ErrorKind::Other, err)
        });

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

            return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
        }

        if let Err(err) = field
            .map_ok(Ok::<_, io::Error>)
            .map_err(|err| {
                error!(%err, %filename, "read upload file data failed");

                io::Error::new(ErrorKind::Other, err)
            })
            .forward(&mut file_sender)
            .await
        {
            let err_msg = err.to_string();

            let _ = file_sender
                .send(Err(io::Error::new(ErrorKind::Other, err)))
                .await;

            return Err((StatusCode::INTERNAL_SERVER_ERROR, err_msg));
        }

        info!(%filename, "read all upload file data done");

        match result_receiver.await {
            Err(err) => {
                error!(%err, %filename, "receive result failed");

                Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }

            Ok(Err(err)) => {
                error!(%err, %filename, "handle upload file command failed");

                Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }

            Ok(Ok(_)) => {
                info!(%filename, "upload file done");

                Ok(())
            }
        }
    }

    #[instrument(skip(self))]
    async fn handle_list_peers(&mut self) -> Result<Json<ListPeersResponse>, StatusCode> {
        let (result_sender, result_receiver) = oneshot::channel();

        if let Err(err) = self
            .command_sender
            .send(Command::ListPeers { result_sender })
            .await
        {
            error!(%err, "send list peers command failed");

            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        let peers = match result_receiver.await {
            Err(err) => {
                error!(%err, "receive result failed");

                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }

            Ok(peers) => {
                info!(?peers, "list peers done");

                peers
                    .into_iter()
                    .map(|peer| ListPeer {
                        peer: peer.to_string(),
                        connected_addrs: vec![],
                    })
                    .collect()
            }
        };

        Ok(Json(ListPeersResponse { peers }))
    }
}
