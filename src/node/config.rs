use std::path::PathBuf;
use std::time::Duration;

use libp2p::identity::Keypair;
use libp2p::pnet::PreSharedKey;
use libp2p::Multiaddr;

#[derive(Debug)]
pub struct Config {
    pub key: Keypair,
    pub index_dir: PathBuf,
    pub store_dir: PathBuf,
    pub handshake_key: PreSharedKey,
    pub refresh_store_interval: Duration,
    pub sync_file_interval: Duration,
    pub enable_relay_behaviour: bool,
    pub relay_server_addr: Option<Multiaddr>,
}
