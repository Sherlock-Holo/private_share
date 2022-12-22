use std::io;
use std::path::PathBuf;

use futures_channel::oneshot::Sender;
use libp2p::PeerId;

#[derive(Debug)]
pub enum Command {
    AddFile {
        file_path: PathBuf,
        result_sender: Sender<io::Result<()>>,
    },

    ListFiles {
        include_peer: bool,
        result_sender: Sender<io::Result<Vec<ListFileDetail>>>,
    },
}

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct ListFileDetail {
    pub filename: String,
    pub hash: String,
    pub downloaded: bool,
    pub peers: Vec<PeerId>,
    pub size: u64,
}
