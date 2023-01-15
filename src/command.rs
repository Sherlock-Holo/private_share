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

    GetBandwidth {
        result_sender: Sender<(u64, u64)>,
    },

    AddPeers {
        peers: Vec<Multiaddr>,
        result_sender: Sender<io::Result<()>>,
    },

    RemovePeers {
        peers: Vec<Multiaddr>,
        result_sender: Sender<io::Result<()>>,
    },
}

impl<FileStream: Stream<Item = io::Result<Bytes>> + Unpin + Send + 'static> Debug
    for Command<FileStream>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug_struct = match self {
            Command::AddFile { file_path, .. } => {
                let mut debug_struct = f.debug_struct("Command::AddFile");

                debug_struct.field("file_path", file_path);

                debug_struct
            }

            Command::ListFiles { include_peer, .. } => {
                let mut debug_struct = f.debug_struct("Command::ListFiles");

                debug_struct.field("include_peer", include_peer);

                debug_struct
            }

            Command::UploadFile { filename, hash, .. } => {
                let mut debug_struct = f.debug_struct("Command::UploadFile");

                debug_struct.field("filename", filename).field("hash", hash);

                debug_struct
            }

            Command::ListPeers { .. } => f.debug_struct("Command::ListPeers"),

            Command::GetBandwidth { .. } => f.debug_struct("Command::GetBandwidth"),

            Command::AddPeers { peers, .. } => {
                let mut debug_struct = f.debug_struct("Command::AddPeers");

                debug_struct.field("peers", peers);

                debug_struct
            }

            Command::RemovePeers { peers, .. } => {
                let mut debug_struct = f.debug_struct("Command::RemovePeers");

                debug_struct.field("peers", peers);

                debug_struct
            }
        };

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
