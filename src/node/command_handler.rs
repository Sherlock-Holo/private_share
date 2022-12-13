use std::collections::HashMap;
use std::io::{Error, ErrorKind, SeekFrom};
use std::path::{Path, PathBuf};

use bytes::BytesMut;
use futures_channel::oneshot::Sender;
use libp2p::request_response::RequestId;
use libp2p::Swarm;
use sha2::digest::FixedOutput;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::fs::{File, OpenOptions};
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tracing::{error, info, instrument};

use crate::command::Command;
use crate::node::behaviour::Behaviour;
use crate::node::FileResponse;

const BUF_SIZE: usize = 1024 * 1024; // 1MiB

pub struct CommandHandler<'a> {
    index_dir: &'a Path,
    store_dir: &'a Path,
    swarm: &'a mut Swarm<Behaviour>,
    file_get_requests: &'a mut HashMap<RequestId, Sender<io::Result<FileResponse>>>,
}

impl<'a> CommandHandler<'a> {
    pub fn new(
        index_dir: &'a Path,
        store_dir: &'a Path,
        swarm: &'a mut Swarm<Behaviour>,
        file_get_requests: &'a mut HashMap<RequestId, Sender<io::Result<FileResponse>>>,
    ) -> Self {
        Self {
            index_dir,
            store_dir,
            swarm,
            file_get_requests,
        }
    }

    #[instrument(skip(self))]
    pub async fn handle_command(mut self, cmd: Command) {
        match cmd {
            Command::GetFile {
                peer_id,
                request,
                response_sender,
            } => {
                let request_id = self
                    .swarm
                    .behaviour_mut()
                    .request_respond
                    .send_request(&peer_id, request);

                info!(%peer_id, %request_id, "send file request to peer done");

                self.file_get_requests.insert(request_id, response_sender);
            }

            Command::AddFile {
                file_path,
                result_sender,
            } => {
                self.handle_get_file_command(file_path, result_sender).await;
            }
        }
    }

    #[instrument(skip(self))]
    async fn handle_get_file_command(
        &mut self,
        file_path: PathBuf,
        result_sender: Sender<io::Result<()>>,
    ) {
        let filename = match file_path.file_name() {
            Some(filename) => filename,
            None => {
                let _ = result_sender.send(Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("path {file_path:?} may not a file"),
                )));

                return;
            }
        };

        info!(?filename, "get filename done");

        let mut file = match File::open(&file_path).await {
            Err(err) => {
                error!(%err, ?file_path, "open file failed");

                let _ = result_sender.send(Err(err));

                return;
            }

            Ok(file) => file,
        };

        let mut buf = BytesMut::with_capacity(BUF_SIZE);
        let mut hasher = Sha256::new();

        loop {
            let n = match file.read_buf(&mut buf).await {
                Err(err) => {
                    error!(%err, "read file failed");

                    let _ = result_sender.send(Err(err));

                    return;
                }

                Ok(n) => n,
            };
            if n == 0 {
                break;
            }

            hasher.update(&buf[..]);
            buf.clear();
        }

        let hash = hasher.finalize_fixed();
        let hash = hex::encode_upper(hash);

        info!(%hash, "calculate file hash done");

        let index_path = self.index_dir.join(hash);
        match fs::metadata(&index_path).await {
            Err(err) if err.kind() != ErrorKind::NotFound => {
                error!(%err, ?index_path, "check index file exists failed");

                let _ = result_sender.send(Err(err));

                return;
            }

            Err(_) => {
                info!(?index_path, "index file not exists, create it");

                if let Err(err) = file.seek(SeekFrom::Start(0)).await {
                    error!(%err, "seek file to start failed");

                    let _ = result_sender.send(Err(err));

                    return;
                }

                let mut index_file = match OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&index_path)
                    .await
                {
                    Err(err) => {
                        error!(%err, ?index_path, "create index file failed");

                        let _ = result_sender.send(Err(err));

                        return;
                    }

                    Ok(index_file) => index_file,
                };

                let copied = match io::copy(&mut file, &mut index_file).await {
                    Err(err) => {
                        error!(%err, ?index_path, "copy file to index store failed");

                        let _ = result_sender.send(Err(err));

                        return;
                    }

                    Ok(n) => n,
                };

                info!(?index_path, copied, "copy file to index store done");
            }

            Ok(metadata) if !metadata.is_file() => {
                error!(
                    ?index_path,
                    "index file is not a file, index store may be broken"
                );

                let _ = result_sender.send(Err(Error::new(
                    ErrorKind::Other,
                    format!("index file {index_path:?} is not a file, index store may be broken"),
                )));

                return;
            }

            Ok(_) => {}
        }

        let store_file_path = self.store_dir.join(filename);

        if let Err(err) = fs::remove_file(&store_file_path).await {
            if err.kind() != ErrorKind::NotFound {
                error!(%err, ?store_file_path, "try remove store file failed");

                let _ = result_sender.send(Err(err));

                return;
            }

            info!(?store_file_path, "remove store file done");
        }

        if let Err(err) = fs::symlink(&index_path, &store_file_path).await {
            error!(%err, ?store_file_path, "create symlink failed");

            let _ = result_sender.send(Err(err));

            return;
        }

        info!(?store_file_path, "create symlink done");
    }
}
