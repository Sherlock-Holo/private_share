use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub files: Vec<ListFile>,
}

#[derive(Debug, Serialize)]
pub struct ListFile {
    pub filename: String,
    pub hash: String,
    pub downloaded: bool,
    pub peers: Vec<String>,
    pub size: String,
}

#[derive(Debug, Deserialize)]
pub struct AddFileRequest {
    pub file_path: String,
}

#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    pub include_peer: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ListPeersResponse {
    pub peers: Vec<ListPeer>,
}

#[derive(Debug, Serialize)]
pub struct ListPeer {
    pub peer: String,
    pub connected_addrs: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetBandwidthQuery {
    pub interval: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct GetBandWidthResponse {
    pub inbound: u64,
    pub outbound: u64,
}

#[derive(Debug, Deserialize)]
pub struct AddPeersRequest {
    pub peers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemovePeersRequest {
    pub peers: Vec<String>,
}
