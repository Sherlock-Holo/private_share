use std::collections::HashSet;
use std::fmt::{Debug, Formatter};
use std::io;
use std::path::PathBuf;

use bytes::Bytes;
use futures_channel::oneshot::Sender;
use futures_util::Stream;
use libp2p::{Multiaddr, PeerId};

pub enum Command<FileStream: Stream<Item = io::Result<Bytes>> + Unpin + Send + 'static> {
    AddFile {
        file_path: PathBuf,
        result_sender: Sender<io::Result<()>>,
    },

    ListFiles {
        include_peer: bool,
        result_sender: Sender<io::Result<Vec<ListFileDetail>>>,
    },

    UploadFile {
        filename: String,
        hash: Option<String>,
        file_stream: FileStream,
        result_sender: Sender<io::Result<()>>,
    },

    ListPeers {
        result_sender: Sender<Vec<(PeerId, HashSet<Multiaddr>)>>,
    },
}

impl<FileStream: Stream<Item = io::Result<Bytes>> + Unpin + Send + 'static> Debug
    for Command<FileStream>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug_struct = f.debug_struct("Command");

        match self {
            Command::AddFile { file_path, .. } => {
                debug_struct.field("file_path", file_path);
            }

            Command::ListFiles { include_peer, .. } => {
                debug_struct.field("include_peer", include_peer);
            }

            Command::UploadFile { filename, hash, .. } => {
                debug_struct.field("filename", filename).field("hash", hash);
            }

            Command::ListPeers { .. } => {}
        }

        debug_struct.finish()
    }
}

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct ListFileDetail {
    pub filename: String,
    pub hash: String,
    pub downloaded: bool,
    pub peers: Vec<PeerId>,
    pub size: u64,
}
