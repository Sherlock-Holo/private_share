use std::collections::HashMap;

use libp2p::{Multiaddr, PeerId, Swarm};
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

    #[instrument(skip(self))]
    pub async fn connect_peer(self, addr: Multiaddr) {
        let peer_id = match PeerId::try_from_multiaddr(&addr) {
            None => {
                error!(%addr, "addr doesn't contain peer id");

                return;
            }

            Some(peer_id) => peer_id,
        };

        if self
            .swarm
            .connected_peers()
            .any(|connected_peer| *connected_peer == peer_id)
        {
            info!(%peer_id, "peer is connected");

            return;
        }

        if let Err(err) = self.swarm.dial(addr.clone()) {
            error!(%err, %addr, "dial peer failed");

            return;
        }

        info!(%peer_id, "peer is dialing");

        self.peer_addr_connecting.insert(peer_id, addr);
    }
}
