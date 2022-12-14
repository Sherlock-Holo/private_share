use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use futures_channel::mpsc::Receiver;
use futures_channel::oneshot::Sender;
use futures_util::stream::Peekable;
use futures_util::StreamExt;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::Boxed;
use libp2p::core::upgrade::{SelectUpgrade, Version};
use libp2p::dns::TokioDnsConfig;
use libp2p::identity::Keypair;
use libp2p::mplex::MplexConfig;
use libp2p::pnet::{PnetConfig, PnetError, PreSharedKey};
use libp2p::request_response::RequestId;
use libp2p::yamux::YamuxConfig;
use libp2p::{noise, tcp, websocket, Multiaddr, PeerId, Swarm, Transport};
use tap::TapFallible;
use tokio::time;
use tokio::time::Interval;
use tracing::{error, info};

use crate::command::Command;
use crate::node::behaviour::{Behaviour, FILE_SHARE_TOPIC};
pub use crate::node::behaviour::{FileRequest, FileResponse};
use crate::node::command_handler::CommandHandler;
use crate::node::config::Config;
use crate::node::event_handler::EventHandler;
use crate::node::peer_connector::PeerConnector;
use crate::node::refresh_store_handler::RefreshStoreHandler;

mod behaviour;
mod command_handler;
pub mod config;
mod event_handler;
mod message;
mod peer_connector;
mod refresh_store_handler;

pub struct Node {
    index_dir: PathBuf,
    store_dir: PathBuf,
    swarm: Swarm<Behaviour>,
    peer_stores: HashMap<PeerId, PeerNodeStore>,
    file_get_requests: HashMap<RequestId, Sender<io::Result<FileResponse>>>,
    peer_addr_receiver: Peekable<Receiver<Multiaddr>>,
    peer_addr_connecting: HashSet<PeerId>,
    command_receiver: Receiver<Command>,
    ticker: Interval,
}

impl Node {
    pub fn new(
        config: Config,
        peer_addr_receiver: Receiver<Multiaddr>,
        command_receiver: Receiver<Command>,
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
            peer_addr_receiver: peer_addr_receiver.peekable(),
            peer_addr_connecting: Default::default(),
            command_receiver,
            ticker: time::interval(config.refresh_store_interval),
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        loop {
            let swarm = &mut self.swarm;
            let mut peer_addr_receiver = Pin::new(&mut self.peer_addr_receiver);
            let command_receiver = &mut self.command_receiver;
            let ticker = &mut self.ticker;

            tokio::select! {
                Some(event) = swarm.next() => {
                    EventHandler::new(
                        &self.index_dir,
                        &self.store_dir,
                        swarm,
                        &mut self.peer_stores,
                        &mut self.file_get_requests,
                        &mut self.peer_addr_connecting
                    ).handle_event(event).await?;
                }

                Some(addr) = peer_addr_receiver.as_mut().peek() => {
                    let addr = addr.clone();

                    PeerConnector::new(swarm, peer_addr_receiver, &mut self.peer_addr_connecting)
                        .connect_peer(addr).await?;
                }

                Some(cmd) = command_receiver.next() => {
                    CommandHandler::new(
                        &self.index_dir,
                        &self.store_dir,
                        swarm,
                        &self.peer_stores,
                        &mut self.file_get_requests
                    ).handle_command(cmd).await
                }

                _ = ticker.tick() => {
                    RefreshStoreHandler::new(&self.store_dir, swarm)
                        .handle_tick(FILE_SHARE_TOPIC.clone()).await?;

                    ticker.reset();
                }
            }
        }
    }
}

pub fn create_transport(
    keypair: Keypair,
    handshake_key: PreSharedKey,
) -> io::Result<Boxed<(PeerId, StreamMuxerBox)>> {
    let transport = websocket::WsConfig::new(TokioDnsConfig::system(tcp::tokio::Transport::new(
        tcp::Config::new().nodelay(true),
    ))?)
    .and_then(move |conn, connected_point| async move {
        let conn = PnetConfig::new(handshake_key)
            .handshake(conn)
            .await
            .tap_err(|err| error!(%err, ?connected_point, "handshake failed"))?;

        info!(?connected_point, "handshake done");

        Ok::<_, PnetError>(conn)
    });

    Ok(transport
        .upgrade(Version::V1)
        .authenticate(noise::NoiseAuthenticated::xx(&keypair).unwrap())
        .multiplex(SelectUpgrade::new(
            YamuxConfig::default(),
            MplexConfig::default(),
        ))
        .timeout(Duration::from_secs(20))
        .boxed())
}

#[derive(Debug, Default)]
pub struct PeerNodeStore {
    files: HashMap<String, String>,
    index: HashSet<String>,
}
