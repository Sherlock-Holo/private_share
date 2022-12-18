use std::collections::{HashMap, HashSet};
use std::io;
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::time::SystemTime;

use bytes::{Bytes, BytesMut};
use futures_channel::oneshot::Sender;
use itertools::Itertools;
use libp2p::gossipsub::GossipsubEvent;
use libp2p::request_response::{
    OutboundFailure, RequestId, RequestResponseEvent, RequestResponseMessage,
};
use libp2p::swarm::SwarmEvent;
use libp2p::{identify, Multiaddr, PeerId, Swarm};
use prost::Message as _;
use tap::TapFallible;
use tokio::fs;
use tokio::fs::File;
use tracing::{debug, error, info, instrument, warn};

use crate::ext::AsyncFileExt;
use crate::node::behaviour::{
    Behaviour, BehaviourEvent, FileRequest, FileResponse, DISCOVER_SHARE_TOPIC, FILE_SHARE_TOPIC,
};
use crate::node::message::{DiscoverMessage, FileMessage, Peer};
use crate::node::PeerNodeStore;

pub struct EventHandler<'a> {
    index_dir: &'a Path,
    store_dir: &'a Path,
    swarm: &'a mut Swarm<Behaviour>,
    peer_stores: &'a mut HashMap<PeerId, PeerNodeStore>,
    file_get_requests: &'a mut HashMap<RequestId, Sender<io::Result<FileResponse>>>,
    peer_addr_connecting: &'a mut HashSet<PeerId>,
}

impl<'a> EventHandler<'a> {
    pub fn new(
        index_dir: &'a Path,
        store_dir: &'a Path,
        swarm: &'a mut Swarm<Behaviour>,
        peer_stores: &'a mut HashMap<PeerId, PeerNodeStore>,
        file_get_requests: &'a mut HashMap<RequestId, Sender<io::Result<FileResponse>>>,
        peer_addr_connecting: &'a mut HashSet<PeerId>,
    ) -> Self {
        Self {
            index_dir,
            store_dir,
            swarm,
            peer_stores,
            file_get_requests,
            peer_addr_connecting,
        }
    }

    #[instrument(err, skip(self, event))]
    pub async fn handle_event<THandlerErr>(
        mut self,
        event: SwarmEvent<BehaviourEvent, THandlerErr>,
    ) -> anyhow::Result<()> {
        match event {
            SwarmEvent::NewListenAddr {
                listener_id,
                address,
            } => {
                info!(?listener_id, %address, "new listen address");

                return Ok(());
            }

            SwarmEvent::Behaviour(event) => match event {
                BehaviourEvent::Gossip(event) => {
                    self.handle_gossip_event(event).await?;

                    info!("handle gossip event done");
                }

                BehaviourEvent::RequestRespond(event) => {
                    self.handle_request_respond_event(event).await?;

                    info!("handle request respond event done");
                }

                BehaviourEvent::Keepalive(_) | BehaviourEvent::Ping(_) => {}

                BehaviourEvent::Identify(event) => {
                    self.handle_identify_event(event).await?;

                    info!("handle identify event done");
                }
            },

            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                self.swarm
                    .behaviour_mut()
                    .gossip
                    .add_explicit_peer(&peer_id);

                if endpoint.is_dialer() {
                    self.peer_addr_connecting.remove(&peer_id);

                    info!(%peer_id, "dial peer done");
                } else {
                    info!(%peer_id, "accept peer done");
                }
            }

            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                self.swarm
                    .behaviour_mut()
                    .gossip
                    .remove_explicit_peer(&peer_id);

                info!(%peer_id, "disconnect with peer");
            }

            SwarmEvent::IncomingConnection { .. } => {}
            SwarmEvent::IncomingConnectionError { .. } => {}
            SwarmEvent::OutgoingConnectionError { .. } => {}
            SwarmEvent::BannedPeer { .. } => {
                unreachable!("we don't ban any peers");
            }
            SwarmEvent::ExpiredListenAddr { .. } => {}
            SwarmEvent::ListenerClosed { .. } => {}
            SwarmEvent::ListenerError { .. } => {}
            SwarmEvent::Dialing(_) => {}
        }

        Ok(())
    }

    #[instrument(err, skip(self, event))]
    async fn handle_gossip_event(&mut self, event: GossipsubEvent) -> anyhow::Result<()> {
        match event {
            GossipsubEvent::Message { message, .. } => {
                if message.topic == FILE_SHARE_TOPIC.hash() {
                    let msg = FileMessage::decode(message.data.as_slice())
                        .tap_err(|err| error!(%err, "decode file message failed"))?;
                    let peer_id = bs58::decode(&msg.peer_id).into_vec().tap_err(
                        |err| error!(%err, peer_id = %msg.peer_id, "decode peer id failed"),
                    )?;
                    let peer_id = PeerId::from_bytes(&peer_id).tap_err(
                        |err| error!(%err, peer_id = %msg.peer_id, "parse peer id failed"),
                    )?;

                    info!(%peer_id, ?msg, "receive file message from peer");

                    let peer_node_store = self
                        .peer_stores
                        .entry(peer_id)
                        .or_insert(PeerNodeStore::default());

                    peer_node_store.files.clear();
                    peer_node_store.index.clear();

                    msg.file_list.into_iter().for_each(|file| {
                        peer_node_store
                            .index
                            .insert(file.hash.clone(), file.file_size);
                        peer_node_store
                            .files
                            .entry(file.filename)
                            .or_insert_with(|| file.hash);
                    });

                    info!(%peer_id, "update peer store done");
                } else if message.topic == DISCOVER_SHARE_TOPIC.hash() {
                    let msg = DiscoverMessage::decode(message.data.as_slice())
                        .tap_err(|err| error!(%err, "decode discover message failed"))?;
                    let peers = msg
                        .peers
                        .into_iter()
                        .map(|peer| {
                            let peer_id = bs58::decode(&peer.peer_id).into_vec().tap_err(
                                |err| error!(%err, peer_id = %peer.peer_id, "decode peer id failed"),
                            )?;
                            let peer_id = PeerId::from_bytes(&peer_id).tap_err(
                                |err| error!(%err, peer_id = %peer.peer_id, "parse peer id failed"),
                            )?;

                            let addr = Multiaddr::try_from(peer.addr)
                                .tap_err(|err| error!(%err, "parse peer addr failed"))?;

                            Ok::<_, anyhow::Error>((peer_id, addr))
                        })
                        .try_collect::<_, Vec<_>, _>()?;

                    info!(?peers, "collect peers done");

                    let behaviour = self.swarm.behaviour_mut();
                    for (peer_id, addr) in peers {
                        if behaviour.request_respond.is_connected(&peer_id) {
                            continue;
                        }

                        behaviour
                            .request_respond
                            .add_address(&peer_id, addr.clone());

                        info!(%peer_id, ?addr, "add peer into request respond");
                    }
                }
            }
            GossipsubEvent::Subscribed { peer_id, topic } => {
                debug!(%peer_id, %topic, "new peer node join the share topic");
            }
            GossipsubEvent::Unsubscribed { peer_id, topic } => {
                debug!(%peer_id, %topic, "peer node leave the share topic");
            }
            GossipsubEvent::GossipsubNotSupported { peer_id } => {
                warn!(%peer_id, "unknown peer join share network failed");
            }
        }

        Ok(())
    }

    #[instrument(err, skip(self, event))]
    async fn handle_request_respond_event(
        &mut self,
        event: RequestResponseEvent<FileRequest, FileResponse>,
    ) -> anyhow::Result<()> {
        match event {
            RequestResponseEvent::Message { peer, message } => {
                self.handle_request_respond_success_event(peer, message)
                    .await?;
            }
            RequestResponseEvent::OutboundFailure {
                peer,
                request_id,
                error: err,
            } => {
                error!(%err, %request_id, %peer, "send file request failed");

                let err = match err {
                    e @ OutboundFailure::DialFailure
                    | e @ OutboundFailure::UnsupportedProtocols => Error::new(ErrorKind::Other, e),
                    e @ OutboundFailure::Timeout => Error::new(ErrorKind::TimedOut, e),
                    e @ OutboundFailure::ConnectionClosed => {
                        Error::new(ErrorKind::ConnectionAborted, e)
                    }
                };

                if let Some(sender) = self.file_get_requests.remove(&request_id) {
                    let _ = sender.send(Err(err));
                }
            }
            RequestResponseEvent::InboundFailure {
                peer,
                request_id,
                error: err,
            } => {
                error!(%err,  %request_id, %peer, "receive file request failed");
            }
            RequestResponseEvent::ResponseSent { peer, request_id } => {
                info!(%peer, %request_id, "send file response success");
            }
        }

        Ok(())
    }

    #[instrument(err, skip(self))]
    async fn handle_request_respond_success_event(
        &mut self,
        peer: PeerId,
        message: RequestResponseMessage<FileRequest, FileResponse>,
    ) -> anyhow::Result<()> {
        match message {
            RequestResponseMessage::Request {
                request_id,
                request,
                channel,
            } => {
                info!(%request_id, %peer, "receive file request from peer");

                let content = self
                    .read_file(
                        &request.filename,
                        &request.hash,
                        request.offset,
                        request.length,
                    )
                    .await?;

                if self
                    .swarm
                    .behaviour_mut()
                    .request_respond
                    .send_response(channel, FileResponse { content })
                    .is_err()
                {
                    error!("send file content failed");
                } else {
                    info!("send file content done");
                }
            }

            RequestResponseMessage::Response {
                request_id,
                response,
            } => {
                info!(%request_id, "receive file response from peer");

                if let Some(sender) = self.file_get_requests.remove(&request_id) {
                    let _ = sender.send(Ok(response));
                }
            }
        }

        Ok(())
    }

    #[instrument(err, skip(self, event))]
    async fn handle_identify_event(&mut self, event: identify::Event) -> anyhow::Result<()> {
        match event {
            identify::Event::Received { peer_id, info } => {
                let peers = info
                    .listen_addrs
                    .into_iter()
                    .map(|addr| Peer {
                        peer_id: peer_id.to_base58(),
                        addr: addr.to_vec(),
                    })
                    .collect::<Vec<_>>();

                let discover_message = DiscoverMessage {
                    peers,
                    discover_time: SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_micros() as _,
                };

                info!(?discover_message, "create discover message done");

                let discover_message = discover_message.encode_to_vec();
                let behaviour = self.swarm.behaviour_mut();

                behaviour
                    .gossip
                    .publish(DISCOVER_SHARE_TOPIC.clone(), discover_message)
                    .tap_err(|err| {
                        error!(
                            %err, topic = ?&*DISCOVER_SHARE_TOPIC,
                            "publish discover message failed"
                        )
                    })?;

                info!(topic = ?&*DISCOVER_SHARE_TOPIC, "publish discover message done");
            }

            identify::Event::Sent { peer_id } => {
                info!(%peer_id, "send identify response to peer done");
            }

            identify::Event::Pushed { .. } | identify::Event::Error { .. } => {}
        }

        Ok(())
    }

    #[instrument(err, skip(self))]
    async fn read_file_directly(
        &mut self,
        filename: &str,
        offset: u64,
        length: u64,
    ) -> io::Result<Option<Bytes>> {
        let file_path = self.store_dir.join(filename);

        let file = match File::open(&file_path).await {
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                error!(%err, ?file_path, "open file failed");

                return Err(err);
            }
            Ok(file) => file,
        };

        info!(?file_path, "open file done");

        let mut buf = BytesMut::zeroed(length as _);

        let read_length = file
            .read_at(&mut buf, offset)
            .await
            .tap_err(|err| error!(%err, offset, length, "read file failed"))?;

        info!(?file_path, offset, read_length, "read file done");

        // Safety: read_length <= length
        unsafe {
            buf.set_len(read_length as _);
        }

        Ok(Some(buf.freeze()))
    }

    #[instrument(err, skip(self))]
    async fn read_file(
        &mut self,
        filename: &str,
        hash: &str,
        offset: u64,
        length: u64,
    ) -> io::Result<Option<Bytes>> {
        let file_path = self.store_dir.join(filename);
        let index_path = self.index_dir.join(hash);

        let file = match fs::read_link(&file_path).await {
            Err(err) if err.kind() != ErrorKind::NotFound => {
                error!(%err, ?file_path, "read file symlink failed");

                return Err(err);
            }

            Err(_) => {
                info!(filename, hash, "symlink file not exists, create it");

                let index_file = match File::open(&index_path).await {
                    Err(err) if err.kind() == ErrorKind::NotFound => {
                        info!(filename, hash, "file not found");

                        return Ok(None);
                    }

                    Err(err) => {
                        error!(%err, ?index_path, "open file failed");

                        return Err(err);
                    }

                    Ok(index_file) => index_file,
                };

                info!(?index_path, "open index file done");

                fs::symlink(&index_path, &file_path).await.tap_err(
                    |err| error!(%err, ?index_path, ?file_path, "create symlink failed"),
                )?;

                info!(filename, hash, "create symlink file done");

                index_file
            }

            Ok(file_index_path) => {
                if file_index_path != index_path {
                    error!(
                        filename,
                        hash,
                        ?file_index_path,
                        "file found, but hash incorrect"
                    );

                    return Err(Error::new(ErrorKind::InvalidData, format!("file {filename} found, but hash {hash} incorrect, symlink {file_index_path:?}")));
                }

                match File::open(&index_path).await {
                    Err(err) if err.kind() == ErrorKind::NotFound => {
                        info!(filename, hash, "file not found");

                        return Ok(None);
                    }

                    Err(err) => {
                        error!(%err, ?index_path, "open file failed");

                        return Err(err);
                    }

                    Ok(index_file) => {
                        info!(?index_path, "open index file done");

                        index_file
                    }
                }
            }
        };

        let mut buf = BytesMut::zeroed(length as _);
        let read_length = file
            .read_at(&mut buf, offset)
            .await
            .tap_err(|err| error!(%err, offset, length, "read data failed"))?;

        info!(
            filename,
            hash, offset, length, read_length, "read file done"
        );

        // Safety: read_length <= length
        unsafe {
            buf.set_len(read_length as _);
        }

        Ok(Some(buf.freeze()))
    }
}
