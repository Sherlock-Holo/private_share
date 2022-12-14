#[derive(prost::Message)]
pub struct FileMessage {
    #[prost(string, tag = "1")]
    pub peer_id: String,

    #[prost(message, repeated, tag = "2")]
    pub file_list: Vec<File>,
}

#[derive(prost::Message)]
pub struct File {
    #[prost(string, tag = "1")]
    pub filename: String,

    #[prost(string, tag = "2")]
    pub hash: String,
}

#[derive(prost::Message)]
pub struct DiscoverMessage {
    #[prost(message, repeated, tag = "1")]
    pub peers: Vec<Peer>,

    /// use for avoid duplicate gossip message
    #[prost(uint64, tag = "2")]
    pub discover_time: u64,
}

#[derive(prost::Message)]
pub struct Peer {
    #[prost(string, tag = "1")]
    pub peer_id: String,

    #[prost(bytes, tag = "2")]
    pub addr: Vec<u8>,
}
