use std::path::PathBuf;
use std::time::Duration;

use libp2p::identity::Keypair;
use libp2p::pnet::PreSharedKey;

#[derive(Debug)]
pub struct Config {
    pub key: Keypair,
    pub index_dir: PathBuf,
    pub store_dir: PathBuf,
    pub handshake_key: PreSharedKey,
    pub refresh_store_interval: Duration,
    pub sync_file_interval: Duration,
}
