use std::collections::HashMap;
use std::path::PathBuf;
use std::{future, io};

use bytes::Bytes;
use futures_channel::mpsc::Receiver;
use futures_channel::oneshot::Sender;
use futures_util::{Stream, StreamExt};
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::Boxed;
use libp2p::core::upgrade::Version;
use libp2p::dns::TokioDnsConfig;
use libp2p::identity::Keypair;
use libp2p::pnet::{PnetConfig, PnetError, PreSharedKey};
use libp2p::request_response::RequestId;
use libp2p::yamux::YamuxConfig;
use libp2p::{noise, tcp, websocket, Multiaddr, PeerId, Swarm, Transport};
use tap::TapFallible;
use tokio::time;
use tokio::time::Interval;
use tokio_util::time::DelayQueue;
use tracing::{error, info};

use crate::command::Command;
use crate::node::behaviour::{Behaviour, FILE_SHARE_TOPIC, MAX_CHUNK_SIZE};
pub use crate::node::behaviour::{FileRequest, FileResponse};
use crate::node::command_handler::CommandHandler;
use crate::node::config::Config;
use crate::node::event_handler::EventHandler;
use crate::node::file_sync::FileSync;
use crate::node::peer_connector::PeerConnector;
use crate::node::refresh_store_handler::RefreshStoreHandler;

mod behaviour;
mod command_handler;
pub mod config;
mod event_handler;
mod file_sync;
mod message;
mod peer_connector;
mod refresh_store_handler;

pub struct Node<FileStream: Stream<Item = io::Result<Bytes>> + Unpin + Send + 'static> {
    index_dir: PathBuf,
    store_dir: PathBuf,
    swarm: Swarm<Behaviour>,
    peer_stores: HashMap<PeerId, PeerNodeStore>,
    file_get_requests: HashMap<RequestId, Sender<io::Result<FileResponse>>>,
    peer_addr_receiver: DelayQueue<Multiaddr>,
    peer_addr_connecting: HashMap<PeerId, Multiaddr>,
    command_receiver: Receiver<Command<FileStream>>,
    refresh_store_ticker: Interval,
    sync_file_ticker: Interval,
}

impl<FileStream: Stream<Item = io::Result<Bytes>> + Unpin + Send + 'static> Node<FileStream> {
    pub fn new(
        config: Config,
        peer_addr_receiver: DelayQueue<Multiaddr>,
        command_receiver: Receiver<Command<FileStream>>,
    ) -> anyhow::Result<Self> {
        let peer_id = config.key.public().to_peer_id();

        info!("local node peer id {}", peer_id);

        let transport = create_transport(config.key.clone(), config.handshake_key)?;
        let behaviour = Behaviour::new(config.key)?;

        let swarm = Swarm::with_tokio_executor(transport, behaviour, peer_id);

        Ok(Self {
            index_dir: config.index_dir,
            store_dir: config.store_dir,
            swarm,
            peer_stores: Default::default(),
            file_get_requests: Default::default(),
            peer_addr_receiver,
            peer_addr_connecting: Default::default(),
            command_receiver,
            refresh_store_ticker: time::interval(config.refresh_store_interval),
            sync_file_ticker: time::interval(config.sync_file_interval),
        })
    }

    pub async fn run(&mut self, addr: Multiaddr) -> anyhow::Result<()> {
        self.swarm
            .listen_on(addr)
            .tap_err(|err| error!(%err, "swarm listen failed"))?;

        let mut sync_file_task = None;
        let mut syncing_files = None;

        loop {
            let swarm = &mut self.swarm;
            let peer_addr_receiver = &mut self.peer_addr_receiver;
            let command_receiver = &mut self.command_receiver;
            let refresh_store_ticker = &mut self.refresh_store_ticker;
            let sync_file_ticker = &mut self.sync_file_ticker;

            match (sync_file_task.take(), &syncing_files) {
                (None, None) => {
                    tokio::select! {
                        Some(event) = swarm.next() => {
                            EventHandler::new(
                                &self.index_dir,
                                &self.store_dir,
                                swarm,
                                &mut self.peer_stores,
                                &mut self.file_get_requests,
                                peer_addr_receiver,
                                &mut self.peer_addr_connecting
                            ).handle_event(event).await?;
                        }

                        Some(addr) = peer_addr_receiver.next() => {
                            PeerConnector::new(swarm, &mut self.peer_addr_connecting)
                                .connect_peer(addr.into_inner()).await?;
                        }

                        Some(cmd) = command_receiver.next() => {
                            CommandHandler::new(
                                &self.index_dir,
                                &self.store_dir,
                                &self.peer_stores,
                            ).handle_command(cmd).await
                        }

                        _ = refresh_store_ticker.tick() => {
                            RefreshStoreHandler::new(&self.store_dir, swarm)
                                .handle_tick(FILE_SHARE_TOPIC.clone()).await?;

                            refresh_store_ticker.reset();
                        }

                        _ = sync_file_ticker.tick() => {
                            let task = FileSync::new(
                                &self.index_dir,
                                &self.store_dir,
                                swarm,
                                &self.peer_stores,
                                &mut self.file_get_requests,
                                None
                            ).sync_files().await?;

                            match task {
                                None => {
                                    sync_file_ticker.reset();
                                }

                                Some(task) => {
                                    info!("start sync files");

                                    sync_file_task.replace(task);
                                }
                            }
                        }
                    }
                }

                (Some(mut task), _) => {
                    tokio::select! {
                        Some(event) = swarm.next() => {
                            EventHandler::new(
                                &self.index_dir,
                                &self.store_dir,
                                swarm,
                                &mut self.peer_stores,
                                &mut self.file_get_requests,
                                peer_addr_receiver,
                                &mut self.peer_addr_connecting
                            ).handle_event(event).await?;
                        }

                        Some(addr) = peer_addr_receiver.next() => {
                            PeerConnector::new(swarm, &mut self.peer_addr_connecting)
                                .connect_peer(addr.into_inner()).await?;
                        }

                        Some(cmd) = command_receiver.next() => {
                            CommandHandler::new(
                                &self.index_dir,
                                &self.store_dir,
                                &self.peer_stores,
                            ).handle_command(cmd).await
                        }

                        // syncing files task is done
                        result = future::poll_fn(|cx| {
                            Pin::new(&mut task).poll(cx)
                        }) => {
                            let result_syncing_files = result.unwrap()?;

                            info!(?result_syncing_files, "sync files task done");

                            // files are still syncing, need continue
                            if let Some(result_syncing_files) = result_syncing_files {
                                info!(?result_syncing_files, "need continue sync files");

                                syncing_files.replace(result_syncing_files);
                            }

                            sync_file_ticker.reset();

                            continue;
                        }
                    }

                    sync_file_task.replace(task);
                }

                // no running syncing files task, but still have files to sync
                (None, Some(_)) => {
                    let task = FileSync::new(
                        &self.index_dir,
                        &self.store_dir,
                        swarm,
                        &self.peer_stores,
                        &mut self.file_get_requests,
                        syncing_files.take(),
                    )
                    .sync_files()
                    .await?;

                    match task {
                        None => {
                            unreachable!(
                                "still have files to sync but no syncing files task return"
                            );
                        }

                        Some(task) => {
                            info!("continue sync files");

                            sync_file_task.replace(task);
                        }
                    }
                }
            }
        }
    }
}

pub fn create_transport(
    keypair: Keypair,
    handshake_key: PreSharedKey,
) -> io::Result<Boxed<(PeerId, StreamMuxerBox)>> {
    let tcp_transport =
        TokioDnsConfig::system(tcp::tokio::Transport::new(tcp::Config::new().nodelay(true)))?
            .and_then(move |conn, connected_point| async move {
                let conn = PnetConfig::new(handshake_key)
                    .handshake(conn)
                    .await
                    .tap_err(|err| error!(%err, ?connected_point, "handshake failed"))?;

                info!(?connected_point, "handshake done");

                Ok::<_, PnetError>(conn)
            });
    let transport = websocket::WsConfig::new(tcp_transport);

    let mut yamux_config = YamuxConfig::default();
    yamux_config.set_max_buffer_size(MAX_CHUNK_SIZE * 2);
    yamux_config.set_receive_window_size((MAX_CHUNK_SIZE * 2) as _);

    Ok(transport
        .upgrade(Version::V1)
        .authenticate(noise::NoiseAuthenticated::xx(&keypair).unwrap())
        .multiplex(yamux_config)
        .boxed())
}

#[derive(Debug, Default)]
pub struct PeerNodeStore {
    files: HashMap<String, String>,
    index: HashMap<String, u64>,
}
