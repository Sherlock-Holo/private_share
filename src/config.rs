use std::net::SocketAddr;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub index_dir: String,
    pub store_dir: String,
    pub secret_key_path: String,
    pub public_key_path: String,
    pub pre_share_key: String,
    pub refresh_interval: String,
    pub sync_file_interval: String,
    pub peer_addrs: Vec<String>,
    pub http_listen: SocketAddr,
    pub swarm_listen: String,
}
