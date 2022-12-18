use std::collections::HashMap;

use libp2p::{Multiaddr, PeerId, Swarm};
use tap::TapFallible;
use tracing::{error, info, instrument};

use crate::node::behaviour::Behaviour;

pub struct PeerConnector<'a> {
    swarm: &'a mut Swarm<Behaviour>,
    peer_addr_connecting: &'a mut HashMap<PeerId, Multiaddr>,
}

impl<'a> PeerConnector<'a> {
    pub fn new(
        swarm: &'a mut Swarm<Behaviour>,
        peer_addr_connecting: &'a mut HashMap<PeerId, Multiaddr>,
    ) -> Self {
        Self {
            swarm,
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

            return Ok(());
        }

        self.swarm
            .dial(addr.clone())
            .tap_err(|err| error!(%err, %addr, "dial peer failed"))?;

        info!(%peer_id, "peer is dialing");

        self.peer_addr_connecting.insert(peer_id, addr);

        Ok(())
    }
}
