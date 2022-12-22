use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::Metadata;
use std::io::{Error, ErrorKind, SeekFrom};
use std::mem;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use bytes::BytesMut;
use futures_channel::oneshot::Sender;
use futures_util::{stream, StreamExt, TryStreamExt};
use libp2p::PeerId;
use sha2::digest::FixedOutput;
use sha2::{Digest, Sha256};
use tap::TapFallible;
use tokio::fs;
use tokio::fs::{File, OpenOptions};
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tracing::{error, info, instrument};

use crate::command::{Command, ListFileDetail};
use crate::node::PeerNodeStore;
use crate::util::collect_filenames;

const BUF_SIZE: usize = 1024 * 1024; // 1MiB

pub struct CommandHandler<'a> {
    index_dir: &'a Path,
    store_dir: &'a Path,
    peer_stores: &'a HashMap<PeerId, PeerNodeStore>,
}

impl<'a> CommandHandler<'a> {
    pub fn new(
        index_dir: &'a Path,
        store_dir: &'a Path,
        peer_stores: &'a HashMap<PeerId, PeerNodeStore>,
    ) -> Self {
        Self {
            index_dir,
            store_dir,
            peer_stores,
        }
    }

    #[instrument(skip(self))]
    pub async fn handle_command(mut self, cmd: Command) {
        match cmd {
            Command::AddFile {
                file_path,
                result_sender,
            } => {
                self.handle_add_file_command(&file_path, result_sender)
                    .await;

                info!(?file_path, "handling add file command");
            }

            Command::ListFiles {
                include_peer,
                result_sender,
            } => {
                self.handle_list_files_command(include_peer, result_sender)
                    .await;

                info!(include_peer, "handle list file command done");
            }
        }
    }

    #[instrument(skip(self))]
    async fn handle_add_file_command(
        &mut self,
        file_path: &Path,
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

        info!(?file_path, "open file done");

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

        let _ = result_sender.send(Ok(()));
    }

    #[instrument(skip(self))]
    async fn handle_list_files_command(
        &mut self,
        include_peer: bool,
        result_sender: Sender<io::Result<Vec<ListFileDetail>>>,
    ) {
        let store_dir = self.store_dir;
        let store_filenames = match collect_filenames(store_dir).await {
            Err(err) => {
                let _ = result_sender.send(Err(err));

                return;
            }

            Ok(filenames) => filenames,
        };

        let mut list_file_details: HashSet<ListFileDetail> =
            match stream::iter(store_filenames.iter())
                .then(|filename| async move {
                    let store_file_path = store_dir.join(filename);

                    let index_file_path = fs::read_link(&store_file_path)
                        .await
                        .tap_err(|err| error!(%err, ?store_file_path, "read symlink failed"))?;

                    info!(?store_file_path, ?index_file_path, "read symlink done");

                    let index_filename = index_file_path.file_name().ok_or_else(|| {
                        error!(?index_file_path, "index file doesn't contain filename");

                        Error::new(
                            ErrorKind::Other,
                            format!("index file {index_file_path:?} doesn't contain filename"),
                        )
                    })?;

                    info!(
                        ?store_file_path,
                        ?index_file_path,
                        ?index_filename,
                        "get index filename done"
                    );

                    let metadata = fs::metadata(&store_file_path)
                        .await
                        .tap_err(|err| error!(%err, "get store file metadata failed"))?;

                    Ok::<_, Error>((filename, index_filename.to_owned(), metadata))
                })
                .map_ok(
                    |(filename, hash, metadata): (&OsString, OsString, Metadata)| ListFileDetail {
                        filename: filename.to_string_lossy().to_string(),
                        hash: hash.to_string_lossy().to_string(),
                        downloaded: true,
                        peers: vec![],
                        size: metadata.size(),
                    },
                )
                .try_collect()
                .await
            {
                Err(err) => {
                    let _ = result_sender.send(Err(err));

                    return;
                }

                Ok(filenames_with_hash) => filenames_with_hash,
            };

        info!(?list_file_details, "collect local files done");

        if !include_peer {
            info!("no need include peer");

            let _ = result_sender.send(Ok(list_file_details.into_iter().collect()));

            return;
        }

        let exists_files = list_file_details
            .iter()
            .map(|detail| (detail.filename.clone(), detail.hash.clone()))
            .collect::<HashSet<_>>();

        let peer_list_file_details = self
            .peer_stores
            .iter()
            .flat_map(|(peer_id, peer_store)| {
                peer_store.files.iter().map(|(filename, hash)| {
                    let size = *peer_store
                        .index
                        .get(hash)
                        .unwrap_or_else(|| panic!("hash {hash} not in index"));

                    (*peer_id, filename, hash, size)
                })
            })
            .filter(|(_, filename, hash, _)| {
                !exists_files.contains(&((*filename).clone(), (*hash).clone()))
            })
            .map(|(peer_id, filename, hash, size)| ListFileDetail {
                filename: filename.to_owned(),
                hash: hash.to_owned(),
                downloaded: false,
                peers: vec![peer_id],
                size,
            });

        list_file_details.extend(peer_list_file_details);

        let mut file_peer_map = HashMap::<_, HashSet<_>>::with_capacity(list_file_details.len());
        for mut list_file_detail in list_file_details {
            let peers = mem::take(&mut list_file_detail.peers);
            match file_peer_map.entry(list_file_detail) {
                entry @ Entry::Vacant(..) => {
                    entry.or_insert(peers.into_iter().collect());
                }
                entry @ Entry::Occupied(..) => {
                    entry.and_modify(|exists_peers| exists_peers.extend(peers));
                }
            }
        }

        let list_file_details = file_peer_map
            .into_iter()
            .map(|(mut list_file_detail, peers)| {
                list_file_detail.peers = peers.into_iter().collect();

                list_file_detail
            })
            .collect::<Vec<_>>();

        info!(?list_file_details, "collect local and peer files done");

        let _ = result_sender.send(Ok(list_file_details.into_iter().collect()));
    }
}
