use std::io;
use std::path::PathBuf;

use futures_channel::oneshot::Sender;
use libp2p::PeerId;

use crate::node::{FileRequest, FileResponse};

#[derive(Debug)]
pub enum Command {
    GetFile {
        peer_id: PeerId,
        request: FileRequest,
        response_sender: Sender<io::Result<FileResponse>>,
    },

    AddFile {
        file_path: PathBuf,
        result_sender: Sender<io::Result<()>>,
    },
}
