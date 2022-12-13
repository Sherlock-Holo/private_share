use std::collections::HashSet;
use std::pin::Pin;

use futures_channel::mpsc::Receiver;
use futures_util::stream::Peekable;
use futures_util::StreamExt;
use libp2p::{Multiaddr, PeerId, Swarm};
use tap::TapFallible;
use tracing::{error, info, instrument};

use crate::node::behaviour::Behaviour;

pub struct PeerConnector<'a> {
    swarm: &'a mut Swarm<Behaviour>,
    peer_addr_receiver: Pin<&'a mut Peekable<Receiver<Multiaddr>>>,
    peer_addr_connecting: &'a mut HashSet<PeerId>,
}

impl<'a> PeerConnector<'a> {
    pub fn new(
        swarm: &'a mut Swarm<Behaviour>,
        peer_addr_receiver: Pin<&'a mut Peekable<Receiver<Multiaddr>>>,
        peer_addr_connecting: &'a mut HashSet<PeerId>,
    ) -> Self {
        Self {
            swarm,
            peer_addr_receiver,
            peer_addr_connecting,
        }
    }

    #[instrument(err, skip(self))]
    pub async fn connect_peer(self, addr: Multiaddr) -> anyhow::Result<()> {
        let peer_id = PeerId::try_from_multiaddr(&addr).ok_or_else(|| {
            error!(%addr, "addr doesn't contain peer id");

            anyhow::anyhow!("addr {} doesn't contain peer id", addr)
        })?;

        if self
            .swarm
            .connected_peers()
            .any(|connected_peer| *connected_peer == peer_id)
        {
            info!(%peer_id, "peer is connected");

            let _ = self.peer_addr_receiver.get_mut().next().await;

            return Ok(());
        }

        if self.peer_addr_connecting.contains(&peer_id) {
            info!(%peer_id, "peer is connecting");

            return Ok(());
        }

        self.swarm
            .dial(addr.clone())
            .tap_err(|err| error!(%err, %addr, "dial peer failed"))?;

        self.peer_addr_connecting.insert(peer_id);

        Ok(())
    }
}
