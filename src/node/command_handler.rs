use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::Metadata;
use std::io::{Error, ErrorKind, SeekFrom};
use std::mem;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use derive_builder::Builder;
use futures_channel::oneshot::Sender;
use futures_util::{stream, Stream, StreamExt, TryStreamExt};
use itertools::Itertools;
use libp2p::bandwidth::BandwidthSinks;
use libp2p::{Multiaddr, PeerId, Swarm};
use rand::distributions::{Alphanumeric, DistString};
use sha2::digest::FixedOutput;
use sha2::{Digest, Sha256};
use tap::TapFallible;
use tokio::fs;
use tokio::fs::{File, OpenOptions};
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::time::DelayQueue;
use tracing::{error, info, instrument, warn};

use crate::command;
use crate::command::{Command, ListFileDetail};
use crate::config::ConfigManager;
use crate::node::behaviour::Behaviour;
use crate::node::PeerNodeStore;
use crate::util::{collect_filenames, create_temp_dir};

const BUF_SIZE: usize = 1024 * 1024; // 1MiB

#[derive(Builder)]
#[builder(pattern = "owned")]
pub struct CommandHandler<'a> {
    index_dir: &'a Path,
    store_dir: &'a Path,
    peer_stores: &'a mut HashMap<PeerId, PeerNodeStore>,
    connected_peer: &'a HashMap<PeerId, HashSet<Multiaddr>>,
    bandwidth_sinks: &'a BandwidthSinks,
    config_manager: &'a mut ConfigManager,
    peer_addr_receiver: &'a mut DelayQueue<Multiaddr>,
    swarm: &'a mut Swarm<Behaviour>,
}

impl<'a> CommandHandler<'a> {
    #[instrument(skip(self))]
    pub async fn handle_command<FileStream, FileGetter>(
        mut self,
        cmd: Command<FileStream, FileGetter>,
    ) where
        FileStream: Stream<Item = io::Result<Bytes>> + Unpin + Send + 'static,
        FileGetter: command::FileGetter + Send + 'static,
    {
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

            Command::UploadFile {
                filename,
                hash,
                file_stream,
                result_sender,
            } => {
                self.handle_upload_file_command(
                    &filename,
                    hash.as_deref(),
                    file_stream,
                    result_sender,
                )
                .await;

                info!(%filename, hash, "uploading file");
            }

            Command::ListPeers { result_sender } => {
                self.handle_list_peers_command(result_sender);

                info!("handle list peers command done");
            }

            Command::GetBandwidth { result_sender } => {
                self.handle_get_bandwidth_command(result_sender);

                info!("handle get bandwidth command done");
            }

            Command::AddPeers {
                peers,
                result_sender,
            } => {
                self.handle_add_peers_command(peers, result_sender).await;

                info!("handle add peers command done");
            }

            Command::RemovePeers {
                peers,
                result_sender,
            } => {
                self.handle_remove_peers_command(peers, result_sender).await;

                info!("handle remove peers command done");
            }

            Command::GetFile {
                filename,
                file_getter,
                result_sender,
            } => {
                self.handle_get_file_command(filename, file_getter, result_sender)
                    .await;

                info!("handle get file command done");
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
            .sorted_by(|a, b| a.filename.cmp(&b.filename))
            .collect::<Vec<_>>();

        info!(?list_file_details, "collect local and peer files done");

        let _ = result_sender.send(Ok(list_file_details.into_iter().collect()));
    }

    #[instrument(skip(self, file_stream))]
    async fn handle_upload_file_command<FileStream>(
        &mut self,
        filename: &str,
        hash: Option<&str>,
        file_stream: FileStream,
        result_sender: Sender<io::Result<()>>,
    ) where
        FileStream: Stream<Item = io::Result<Bytes>> + Unpin + Send + 'static,
    {
        let store_path = self.store_dir.join(filename);
        if let Some(hash) = hash {
            info!("command has hash");

            let index_path = self.index_dir.join(hash);

            match fs::metadata(&index_path).await {
                Err(err) if err.kind() != ErrorKind::NotFound => {
                    error!(%err, ?index_path, "get index file metadata failed");

                    let _ = result_sender.send(Err(err));

                    return;
                }

                Ok(_) => {
                    info!(?index_path, "index file exists");

                    if let Err(err) = fs::symlink(&index_path, &store_path).await {
                        if err.kind() == ErrorKind::AlreadyExists {
                            info!(?store_path, ?index_path, "store file exists");

                            let _ = result_sender.send(Ok(()));

                            return;
                        }

                        error!(%err, ?store_path, ?index_path, "create symlink failed");

                        let _ = result_sender.send(Err(err));

                        return;
                    }

                    info!(?store_path, ?index_path, "create symlink done");

                    let _ = result_sender.send(Ok(()));

                    return;
                }

                Err(_) => {}
            }
        }

        let filename = filename.to_owned();
        let hash = hash.map(ToOwned::to_owned);
        let index_dir = self.index_dir.to_owned();
        let store_dir = self.store_dir.to_owned();
        tokio::spawn(async move {
            upload_file(
                &filename,
                hash.as_deref(),
                index_dir,
                store_dir,
                file_stream,
                result_sender,
            )
            .await;
        });

        info!("start upload file task");
    }

    #[instrument(skip(self))]
    fn handle_list_peers_command(
        &mut self,
        result_sender: Sender<Vec<(PeerId, HashSet<Multiaddr>)>>,
    ) {
        let peers = self
            .connected_peer
            .iter()
            .map(|(peer, addrs)| (*peer, addrs.clone()))
            .collect();

        info!(?peers, "collect connected peers done");

        let _ = result_sender.send(peers);
    }

    #[instrument(skip(self))]
    fn handle_get_bandwidth_command(&mut self, result_sender: Sender<(u64, u64)>) {
        let inbound = self.bandwidth_sinks.total_inbound();
        let outbound = self.bandwidth_sinks.total_outbound();

        info!(inbound, outbound, "get inbound and outbound done");

        let _ = result_sender.send((inbound, outbound));
    }

    #[instrument(skip(self))]
    async fn handle_add_peers_command(
        &mut self,
        mut peers: Vec<Multiaddr>,
        result_sender: Sender<io::Result<()>>,
    ) {
        let mut config = self.config_manager.load();
        peers = peers
            .into_iter()
            .filter(|peer| {
                if config.peer_addrs.contains(&peer.to_string()) {
                    info!(%peer, "peer is added, ignore");

                    false
                } else {
                    true
                }
            })
            .collect::<Vec<_>>();
        if peers.is_empty() {
            let _ = result_sender.send(Ok(()));

            return;
        }

        for peer in peers {
            info!(%peer, "add peer to peer addr receiver done");

            config.to_mut().peer_addrs.push(peer.to_string());
            self.peer_addr_receiver.insert(peer, Duration::from_secs(0));
        }

        let result = self
            .config_manager
            .swap(Cow::Owned(config.into_owned()))
            .await
            .tap_ok(|_| info!("swap config done"))
            .tap_err(|err| error!(%err, "swap config failed"))
            .map(|_| ());

        let _ = result_sender.send(result);
    }

    #[instrument(skip(self))]
    async fn handle_remove_peers_command(
        &mut self,
        mut peers: Vec<Multiaddr>,
        result_sender: Sender<io::Result<()>>,
    ) {
        let mut config = self.config_manager.load();
        peers = peers
            .into_iter()
            .filter(|peer| {
                if !config.peer_addrs.contains(&peer.to_string()) {
                    info!(%peer, "peer not exists, ignore");

                    false
                } else {
                    true
                }
            })
            .collect::<Vec<_>>();
        if peers.is_empty() {
            let _ = result_sender.send(Ok(()));

            return;
        }

        for peer in &peers {
            let peer_id = match PeerId::try_from_multiaddr(peer) {
                None => {
                    error!(%peer, "peer doesn't contain peer id");

                    let _ = result_sender.send(Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("peer {peer} doesn't contain peer id"),
                    )));

                    return;
                }

                Some(peer_id) => peer_id,
            };

            info!(%peer_id, %peer, "get peer id from peer done");

            // disconnect peer and remove from each behaviour
            let _ = self.swarm.disconnect_peer_id(peer_id);
            let behaviour = self.swarm.behaviour_mut();
            behaviour.gossip.remove_explicit_peer(&peer_id);
            behaviour.request_respond.remove_address(&peer_id, peer);

            // remove peer from config
            config
                .to_mut()
                .peer_addrs
                .retain(|exist_peer| *exist_peer != peer.to_string());

            // remove peer store info
            self.peer_stores.remove(&peer_id);

            info!(%peer_id, %peer, "remove peer done");
        }

        let result = self
            .config_manager
            .swap(Cow::Owned(config.into_owned()))
            .await
            .tap_ok(|_| info!("swap config done"))
            .tap_err(|err| error!(%err, "swap config failed"))
            .map(|_| ());

        let _ = result_sender.send(result);
    }

    #[instrument(skip(self, file_getter, result_sender))]
    async fn handle_get_file_command<FileGetter>(
        &mut self,
        filename: String,
        file_getter: FileGetter,
        result_sender: Sender<io::Result<Option<FileGetter::FileContent>>>,
    ) where
        FileGetter: command::FileGetter + Send + 'static,
    {
        let store_dir = self.store_dir;
        let store_filenames = match collect_filenames(store_dir).await {
            Err(err) => {
                let _ = result_sender.send(Err(err));

                return;
            }

            Ok(filenames) => filenames,
        };

        let filename = OsString::from(filename);
        if !store_filenames.contains(&filename) {
            error!(?filename, "file not found");

            let _ = result_sender.send(Ok(None));

            return;
        }

        info!(?filename, "check file done and file exists");

        let result = file_getter.get_file(&store_dir.join(filename)).await;
        let _ = result_sender.send(result.map(Some));
    }
}

#[instrument(skip(file_stream))]
async fn upload_file<FileStream: Stream<Item = io::Result<Bytes>> + Unpin + Send + 'static>(
    filename: &str,
    hash: Option<&str>,
    index_dir: PathBuf,
    store_dir: PathBuf,
    mut file_stream: FileStream,
    result_sender: Sender<io::Result<()>>,
) {
    let mut tmp_path = match create_temp_dir(&index_dir).await {
        Err(err) => {
            let _ = result_sender.send(Err(err));

            return;
        }

        Ok(tmp_path) => tmp_path,
    };

    info!(?tmp_path, "create temp index dir done");

    let mut hasher = Sha256::new();
    let tmp_filename = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
    tmp_path.push(format!(".upload.{tmp_filename}"));

    info!(?tmp_path, "generate upload temp file path done");

    let mut upload_file = match OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp_path)
        .await
    {
        Err(err) => {
            error!(%err, ?tmp_path, "create upload temp file failed");

            let _ = result_sender.send(Err(err));

            return;
        }

        Ok(file) => file,
    };

    while let Some(result) = file_stream.next().await {
        let mut data = match result {
            Err(err) => {
                error!("read file content failed");

                let _ = result_sender.send(Err(err));

                return;
            }

            Ok(data) => data,
        };

        hasher.update(&data);

        if let Err(err) = upload_file.write_all_buf(&mut data).await {
            error!(%err, "write data to upload temp file failed");

            let _ = result_sender.send(Err(err));

            return;
        }
    }

    let hash_result = hex::encode_upper(hasher.finalize_fixed());
    if let Some(hash) = hash {
        if hash_result != hash {
            warn!(%hash_result, "hash result is not equal request hash");
        }
    }

    info!(%hash_result, "write data to upload temp file done");

    let index_path = index_dir.join(hash_result);

    match fs::rename(&tmp_path, &index_path).await {
        Err(err) if err.kind() != ErrorKind::AlreadyExists => {
            error!(%err, ?index_path, ?tmp_path, "move upload temp file to index dir failed");

            let _ = result_sender.send(Err(err));

            return;
        }

        Err(_) => info!(?index_path, ?tmp_path, "same hash index file exists"),
        Ok(_) => info!(
            ?index_path,
            ?tmp_path,
            "move upload temp file to index dir done"
        ),
    }

    let store_path = store_dir.join(filename);
    match fs::symlink(&index_path, &store_path).await {
        Err(err) if err.kind() != ErrorKind::AlreadyExists => {
            error!(%err, ?index_path, ?store_path, "create symlink failed");

            let _ = result_sender.send(Err(err));

            return;
        }

        Err(_) => info!(?index_path, ?store_path, "same filename store file exists"),
        Ok(_) => info!(?index_path, ?store_path, "create symlink done"),
    }

    let _ = result_sender.send(Ok(()));
}
