use std::borrow::Cow;
use std::fmt::{Debug, Display};
use std::future::ready;
use std::io;
use std::io::ErrorKind;
use std::time::Duration;

use axum::body::BoxBody;
use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket};
use axum::extract::{
    ConnectInfo, DefaultBodyLimit, Multipart, Path, Query, State, WebSocketUpgrade,
};
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::{body, Json, Router};
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use byte_unit::Byte;
use bytes::Bytes;
use futures_channel::mpsc::Sender;
use futures_channel::{mpsc, oneshot};
use futures_util::{pin_mut, SinkExt, Stream, StreamExt, TryStreamExt};
use http::{Request, Response, StatusCode, Uri};
use http_dir::ResponseBody;
use itertools::Itertools;
use libp2p::Multiaddr;
use tap::{Tap, TapFallible};
use tokio::{select, time};
use tokio_stream::wrappers::IntervalStream;
use tower::Service;
use tracing::{error, info, instrument, warn};

pub use self::addr_incoming::MultiAddrListener;
use self::dlna::TV;
use self::file::FileGetter;
use self::response::*;
use self::socket_addr_peer::SocketAddrPeer;
use self::static_router::StaticRouter;
use crate::command::Command;

mod addr_incoming;
mod dlna;
mod file;
mod response;
mod socket_addr_peer;
mod static_resources;
mod static_router;

const API_PREFIX: &str = "/api";
const UI_PREFIX: &str = "/ui";
const LIST_FILES_PATH: &str = "/list_files";
const ADD_FILE_PATH: &str = "/add_file";
const UPLOAD_FILE_PATH: &str = "/upload_file";
const LIST_PEERS_PATH: &str = "/list_peers";
const GET_BANDWIDTH_PATH: &str = "/get_bandwidth";
const ADD_PEERS_PATH: &str = "/add_peers";
const REMOVE_PEERS_PATH: &str = "/remove_peers";
const GET_FILE_PATH: &str = "/get_file/:filename";
const LIST_TV_PATH: &str = "/list_tv";
const PLAY_TV_PATH: &str = "/play_tv/:encoded_tv_url/:filename";

type UploadFileReceiver = impl Stream<Item = io::Result<Bytes>> + Unpin + Send + 'static;

#[derive(Debug, Clone)]
pub struct Server {
    command_sender: Sender<Command<UploadFileReceiver, FileGetter>>,
}

impl Server {
    pub fn new(command_sender: Sender<Command<UploadFileReceiver, FileGetter>>) -> Self {
        Self { command_sender }
    }

    pub async fn listen(self, incoming: MultiAddrListener) -> anyhow::Result<()> {
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
                .route(
                    GET_BANDWIDTH_PATH,
                    get(|State(mut server): State<Server>, query, ws| async move {
                        server.handle_get_bandwidth(query, ws).await
                    }),
                )
                .route(
                    ADD_PEERS_PATH,
                    post(|State(mut server): State<Server>, req| async move {
                        server.handle_add_peers(req).await
                    }),
                )
                .route(
                    REMOVE_PEERS_PATH,
                    post(|State(mut server): State<Server>, req| async move {
                        server.handle_remove_peers(req).await
                    }),
                )
                .route(
                    GET_FILE_PATH,
                    get(
                        |State(mut server): State<Server>, path, request| async move {
                            server.handle_get_file(request, path).await
                        },
                    ),
                )
                .route(
                    LIST_TV_PATH,
                    get(|State(mut server): State<Server>, query, ws| async move {
                        server.handle_list_tv(query, ws).await
                    }),
                )
                .route(
                    PLAY_TV_PATH,
                    post(
                        |State(mut server): State<Server>, url_path, connect_info| async move {
                            server.handle_play_video(url_path, connect_info).await
                        },
                    ),
                )
                .layer(DefaultBodyLimit::disable());

        let router = Router::new()
            .nest(API_PREFIX, api_router)
            .nest(UI_PREFIX, StaticRouter::default().into())
            .fallback(|| ready(Redirect::temporary("/ui")))
            .with_state(self);

        axum::Server::builder(incoming)
            .serve(router.into_make_service_with_connect_info::<SocketAddrPeer>())
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
                        peer: peer.0.to_string(),
                        connected_addrs: peer.1.into_iter().map(|addr| addr.to_string()).collect(),
                    })
                    .collect()
            }
        };

        Ok(Json(ListPeersResponse { peers }))
    }

    #[instrument(skip(self))]
    async fn handle_get_bandwidth(
        &mut self,
        Query(get_bandwidth_query): Query<GetBandwidthQuery>,
        ws: WebSocketUpgrade,
    ) -> Response<BoxBody> {
        let interval = get_bandwidth_query
            .interval
            .map(|interval| Duration::from_millis(interval as _))
            .unwrap_or_else(|| Duration::from_secs(1));
        let mut this = self.clone();

        ws.on_upgrade(move |mut websocket| async move {
            let mut interval_stream = IntervalStream::new(time::interval(interval));

            loop {
                select! {
                    _ = interval_stream.next() => {
                        this.send_bandwidth(&mut websocket).await;
                    }

                    message = websocket.recv() => {
                        if let Some(true) = handle_websocket_in_message(&mut websocket, message).await {
                            let _ = websocket
                                .close()
                                .await
                                .tap_err(|err| error!(%err, "graceful close websocket failed"))
                                .tap(|_| info!("graceful close websocket done"));

                            return;
                        }
                    }
                }
            }
        })
    }

    async fn send_bandwidth(&mut self, websocket: &mut WebSocket) {
        let (result_sender, result_receiver) = oneshot::channel();

        if let Err(err) = self
            .command_sender
            .send(Command::GetBandwidth { result_sender })
            .await
        {
            error!(%err, "send get bandwidth command failed");

            websocket_close_with_err(websocket, err).await;

            return;
        }

        info!("send get bandwidth command done");

        let (inbound, outbound) = match result_receiver.await {
            Err(err) => {
                error!(%err, "receive result failed");

                websocket_close_with_err(websocket, err).await;

                return;
            }

            Ok(bandwidth) => bandwidth,
        };

        info!(inbound, outbound, "get bandwidth done");

        let response = GetBandWidthResponse { inbound, outbound };
        let response = match serde_json::to_string(&response) {
            Err(err) => {
                error!(%err, ?response, "marshal response failed");

                websocket_close_with_err(websocket, err).await;

                return;
            }

            Ok(resp) => resp,
        };

        info!(%response, "marshal response done");

        if let Err(err) = websocket.send(Message::Text(response)).await {
            error!(%err, inbound, outbound, "send bandwidth failed");
        }
    }

    #[instrument(skip(self))]
    async fn handle_add_peers(
        &mut self,
        Json(req): Json<AddPeersRequest>,
    ) -> Result<(), (StatusCode, String)> {
        let peers = match req
            .peers
            .iter()
            .map(|peer| Multiaddr::try_from(peer.as_str()))
            .try_collect::<_, Vec<_>, _>()
        {
            Err(err) => {
                error!(%err, ?req, "parse peers to multi addr failed");

                return Err((StatusCode::BAD_REQUEST, err.to_string()));
            }

            Ok(peers) => peers,
        };

        info!(?peers, "parse peers done");

        let (result_sender, result_receiver) = oneshot::channel();

        if let Err(err) = self
            .command_sender
            .send(Command::AddPeers {
                peers,
                result_sender,
            })
            .await
        {
            error!(%err, "send add peers command failed");

            return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
        }

        match result_receiver.await {
            Err(err) => {
                error!(%err, "receive result failed");

                Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }

            Ok(Err(err)) => {
                error!(%err, "add peers failed");

                Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }

            Ok(Ok(_)) => Ok(()),
        }
    }

    #[instrument(skip(self))]
    async fn handle_remove_peers(
        &mut self,
        Json(req): Json<RemovePeersRequest>,
    ) -> Result<(), (StatusCode, String)> {
        let peers = match req
            .peers
            .iter()
            .map(|peer| Multiaddr::try_from(peer.as_str()))
            .try_collect::<_, Vec<_>, _>()
        {
            Err(err) => {
                error!(%err, ?req, "parse peers to multi addr failed");

                return Err((StatusCode::BAD_REQUEST, err.to_string()));
            }

            Ok(peers) => peers,
        };

        info!(?peers, "parse peers done");

        let (result_sender, result_receiver) = oneshot::channel();

        if let Err(err) = self
            .command_sender
            .send(Command::RemovePeers {
                peers,
                result_sender,
            })
            .await
        {
            error!(%err, "send remove peers command failed");

            return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
        }

        match result_receiver.await {
            Err(err) => {
                error!(%err, "receive result failed");

                Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }

            Ok(Err(err)) => {
                error!(%err, "remove peers failed");

                Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }

            Ok(Ok(_)) => Ok(()),
        }
    }

    #[instrument(skip(self))]
    async fn handle_get_file(
        &mut self,
        request: Request<body::Body>,
        Path(filename): Path<String>,
    ) -> Result<Response<ResponseBody>, (StatusCode, String)> {
        let (result_sender, result_receiver) = oneshot::channel();

        if let Err(err) = self
            .command_sender
            .send(Command::GetFile {
                filename: filename.clone(),
                file_getter: FileGetter::default(),
                result_sender,
            })
            .await
        {
            error!(%err, "send get file command failed");

            return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
        }

        let mut file_content = match result_receiver.await {
            Err(err) => {
                error!(%err, "receive result failed");

                return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
            }

            Ok(Err(err)) => {
                error!(%err, "get file failed");

                return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
            }

            Ok(Ok(None)) => {
                error!(%filename, "file not found");

                return Err((StatusCode::NOT_FOUND, String::new()));
            }

            Ok(Ok(Some(file_content))) => file_content,
        };

        info!(%filename, "get file done");

        let request = Request::from_parts(request.into_parts().0, ());

        file_content.call(request).await.map_err(|err| {
            error!(%err, "send file content failed");

            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        })
    }

    #[instrument(skip(self))]
    async fn handle_list_tv(
        &mut self,
        Query(list_tv_query): Query<ListTVQuery>,
        ws: WebSocketUpgrade,
    ) -> Response<BoxBody> {
        let mut timeout = Duration::from_millis(list_tv_query.timeout.unwrap_or(10000) as _);

        ws.on_upgrade(move |mut websocket| async move {
            loop {
                let tv_stream = match dlna::list_tv(timeout).await {
                    Err(err) => {
                        error!(%err, "list tv failed");

                        websocket_close_with_err(&mut websocket, err).await;

                        return;
                    }

                    Ok(tv_stream) => tv_stream,
                };

                info!("get list tv stream done");

                pin_mut!(tv_stream);

                loop {
                    select! {
                        msg = websocket.recv() => {
                            let msg = match msg {
                                None => return,
                                Some(Err(err)) => {
                                    error!(%err, "receive websocket message failed");

                                    websocket_close_with_err(&mut websocket, err).await;

                                    return;
                                }

                                Some(Ok(msg)) => msg
                            };

                            let relist_req = match msg {
                                Message::Close(_) => {
                                    if let Err(err) = websocket.close().await {
                                        error!(%err, "close websocket failed");
                                    }

                                    return;
                                }

                                Message::Ping(_) | Message::Pong(_) | Message::Binary(_) => {
                                    warn!("unexpected websocket binary, ping or pong message");

                                    continue;
                                }

                                Message::Text(relist_req) => {
                                    match serde_json::from_str::<ReListTV>(&relist_req) {
                                        Err(err) => {
                                            error!(%err, %relist_req, "unmarshal relist tv request failed");

                                            websocket_close_with_err(&mut websocket, err).await;

                                            return;
                                        }

                                        Ok(relist_req) => {
                                            info!(?relist_req, "unmarshal relist tv request done");

                                            relist_req
                                        }
                                    }
                                }
                            };

                            timeout = Duration::from_millis(relist_req.timeout.unwrap_or(10000) as _);

                            break;
                        }

                        Some(tv) = tv_stream.next() => {
                            let tv = match tv {
                                Err(err) => {
                                    error!(%err, "get next tv failed");

                                    websocket_close_with_err(&mut websocket, err).await;

                                    return;
                                }

                                Ok(tv) => tv
                            };

                            info!(?tv, "get next tv done");

                            let response = ListTVResponse {
                                friend_name: tv.friend_name().to_string(),
                                encoded_url: BASE64_STANDARD.encode(tv.url()),
                            };
                            let response = match serde_json::to_string(&response) {
                                Err(err) => {
                                    error!(%err, ?response, "marshal list tv response failed");

                                    websocket_close_with_err(&mut websocket, err).await;

                                    return;
                                }

                                Ok(resp) => resp,
                            };

                            info!(%response, "marshal list tv response done");

                            if let Err(err) = websocket.send(Message::Text(response)).await {
                                error!(%err, "send list tv response failed");

                                websocket_close_with_err(&mut websocket, err).await;

                                return;
                            }

                            info!("send list tv response done");
                        }
                    }
                }
            }
        })
    }

    #[instrument(skip(self))]
    async fn handle_play_video(
        &mut self,
        Path((encoded_tv_url, filename)): Path<(String, String)>,
        ConnectInfo(addr_peer): ConnectInfo<SocketAddrPeer>,
    ) -> Result<(), (StatusCode, String)> {
        let tv_url = match BASE64_STANDARD.decode(&encoded_tv_url) {
            Err(err) => {
                error!(%err, %encoded_tv_url, "parse b64 encoded tv url failed");

                return Err((StatusCode::BAD_REQUEST, err.to_string()));
            }

            Ok(tv_url) => match String::from_utf8(tv_url) {
                Err(err) => {
                    error!(%err, %encoded_tv_url, "encoded tv url is not valid utf8 string");

                    return Err((StatusCode::BAD_REQUEST, err.to_string()));
                }

                Ok(tv_url) => tv_url,
            },
        };

        let tv_url = match tv_url.parse::<Uri>() {
            Err(err) => {
                error!(%err, %tv_url, "parse tv url failed");

                return Err((StatusCode::BAD_REQUEST, err.to_string()));
            }

            Ok(tv_url) => tv_url,
        };

        info!(%tv_url, "parse tv url done");

        let tv = match TV::from_url(tv_url.clone()).await {
            Err(err) => {
                error!(%err, %tv_url, "get tv from url failed");

                return Err((StatusCode::BAD_REQUEST, err.to_string()));
            }

            Ok(None) => {
                error!(%tv_url, "tv not exists");

                return Err((StatusCode::NOT_FOUND, format!("tv {tv_url} not exists")));
            }

            Ok(Some(tv)) => tv,
        };

        info!(?tv, %tv_url, "get tv from url done");

        let get_file_url_path = format!(
            "{API_PREFIX}/{}",
            GET_FILE_PATH.replace(":filename", &filename)
        );

        let port = addr_peer
            .local
            .ok_or_else(|| {
                error!("can't get local tcp port");

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "can't get local tcp port".to_string(),
                )
            })?
            .port();

        match tv.play(port, &get_file_url_path).await {
            Err(err) => {
                error!(%err, port, %get_file_url_path, "play video failed");

                Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
            }

            Ok(_) => {
                info!(port, %get_file_url_path, "play video done");

                Ok(())
            }
        }
    }
}

#[instrument]
async fn handle_websocket_in_message(
    websocket: &mut WebSocket,
    message: Option<Result<Message, axum::Error>>,
) -> Option<bool> {
    let message = match message.transpose() {
        Err(err) => {
            error!(%err, "receive websocket message failed");

            websocket_close_with_err(websocket, err).await;

            return None;
        }

        Ok(None) => {
            info!("websocket is closed");

            return Some(true);
        }

        Ok(Some(message)) => message,
    };

    match message {
        Message::Text(_) | Message::Binary(_) => {
            warn!(?message, "unexpected websocket message");

            None
        }
        Message::Ping(_) | Message::Pong(_) => {
            info!(?message, "receive websocket ping or pong message");

            None
        }

        Message::Close(reason) => {
            info!(?reason, "websocket is closed");

            Some(true)
        }
    }
}

#[instrument]
async fn websocket_close_with_err<E: Debug + Display>(websocket: &mut WebSocket, err: E) {
    if let Err(err) = websocket
        .send(Message::Close(Some(CloseFrame {
            code: close_code::ERROR,
            reason: Cow::from(err.to_string()),
        })))
        .await
    {
        error!(%err, "send close frame failed");
    }
}
