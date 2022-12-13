use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::pin::Pin;

use futures_channel::mpsc::Receiver;
use futures_channel::oneshot::Sender;
use futures_util::stream::Peekable;
use futures_util::StreamExt;
use libp2p::request_response::RequestId;
use libp2p::{Multiaddr, PeerId, Swarm};

use crate::command::Command;
use crate::node::behaviour::Behaviour;
pub use crate::node::behaviour::{FileRequest, FileResponse};
use crate::node::command_handler::CommandHandler;
use crate::node::event_handler::EventHandler;
use crate::node::peer_connector::PeerConnector;

mod behaviour;
mod command_handler;
mod event_handler;
mod message;
mod peer_connector;

pub struct Node {
    index_dir: PathBuf,
    store_dir: PathBuf,
    swarm: Swarm<Behaviour>,
    peer_stores: HashMap<PeerId, PeerNodeStore>,
    file_get_requests: HashMap<RequestId, Sender<io::Result<FileResponse>>>,
    peer_addr_receiver: Peekable<Receiver<Multiaddr>>,
    peer_addr_connecting: HashSet<PeerId>,
    command_receiver: Receiver<Command>,
}

impl Node {
    pub async fn run(&mut self) -> anyhow::Result<()> {
        loop {
            let swarm = &mut self.swarm;
            let mut peer_addr_receiver = Pin::new(&mut self.peer_addr_receiver);
            let command_receiver = &mut self.command_receiver;

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
                        &mut self.file_get_requests
                    ).handle_command(cmd).await
                }
            }
        }

        todo!()
    }
}

#[derive(Debug, Default)]
pub struct PeerNodeStore {
    files: HashMap<String, String>,
    index: HashSet<String>,
}
