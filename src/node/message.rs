#[derive(prost::Message)]
pub struct Message {
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
