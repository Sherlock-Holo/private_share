use std::collections::{HashMap, HashSet};
use std::io;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_channel::oneshot;
use futures_channel::oneshot::Sender;
use futures_util::{StreamExt, TryStreamExt};
use libp2p::request_response::RequestId;
use libp2p::{PeerId, Swarm};
use rand::prelude::SliceRandom;
use tap::TapFallible;
use tokio::fs;
use tokio::fs::{File, OpenOptions};
use tokio::task::JoinHandle;
use tracing::{error, info, instrument};

use crate::ext::{AsyncFileExt, IterExt};
use crate::node::behaviour::Behaviour;
use crate::node::{FileRequest, FileResponse, PeerNodeStore};
use crate::util::collect_filenames;

/// 8MiB
const MAX_FILE_CHUNK_SIZE: u64 = 8 * 1024 * 1024;

/// limit the number of max concurrent sync tasks
const MAX_CONCURRENT_SYNC_TASKS: usize = 16;

pub type SyncFilesResult = anyhow::Result<Option<SyncFileTask>>;
pub type SyncFileTask = JoinHandle<anyhow::Result<Option<HashMap<String, HashFile>>>>;

pub struct FileSync<'a> {
    index_dir: &'a Path,
    store_dir: &'a Path,
    swarm: &'a mut Swarm<Behaviour>,
    peer_stores: &'a HashMap<PeerId, PeerNodeStore>,
    file_get_requests: &'a mut HashMap<RequestId, Sender<io::Result<FileResponse>>>,
    syncing_files: Option<HashMap<String, HashFile>>,
}

impl<'a> FileSync<'a> {
    pub fn new(
        index_dir: &'a Path,
        store_dir: &'a Path,
        swarm: &'a mut Swarm<Behaviour>,
        peer_stores: &'a HashMap<PeerId, PeerNodeStore>,
        file_get_requests: &'a mut HashMap<RequestId, Sender<io::Result<FileResponse>>>,
        syncing_files: Option<HashMap<String, HashFile>>,
    ) -> Self {
        Self {
            index_dir,
            store_dir,
            swarm,
            peer_stores,
            file_get_requests,
            syncing_files,
        }
    }

    #[instrument(err, skip(self))]
    pub async fn sync_files(mut self) -> SyncFilesResult {
        let mut need_sync_files = match self.syncing_files.take() {
            None => {
                let need_sync_filenames = match self.need_sync().await? {
                    None => {
                        info!("no need sync");

                        return Ok(None);
                    }

                    Some(need_sync_filenames) => need_sync_filenames,
                };

                info!(?need_sync_filenames, "need sync files");

                match fs::remove_dir_all(self.index_dir.join(".tmp")).await {
                    Err(err) if err.kind() != ErrorKind::NotFound => {
                        error!(%err, "remove tmp dir at first done");

                        return Err(err.into());
                    }

                    Err(_) => {}
                    Ok(_) => {
                        info!("remove tmp dir at first done");
                    }
                }

                need_sync_filenames
            }

            Some(need_sync_filenames) => need_sync_filenames,
        };

        let mut remaining_task_number = MAX_CONCURRENT_SYNC_TASKS;
        let mut futs = Vec::with_capacity(MAX_CONCURRENT_SYNC_TASKS.min(need_sync_files.len()));
        for (hash, hash_file) in need_sync_files.iter_mut() {
            if remaining_task_number == 0 {
                break;
            }

            let tmp_index_file = Arc::new(self.create_or_open_temp_index_file(hash).await?);

            info!(%hash, "create temp index file done");

            let mut offset = hash_file.syncing_offset;
            while offset < hash_file.size {
                let length = MAX_FILE_CHUNK_SIZE;

                let (sender, receiver) = oneshot::channel();

                let file_request = FileRequest {
                    filename: hash_file.filenames[0].clone(),
                    hash: hash.clone(),
                    offset,
                    length,
                };

                let peer_id = hash_file.peers.choose(&mut rand::thread_rng()).unwrap();
                let request_id = self
                    .swarm
                    .behaviour_mut()
                    .request_respond
                    .send_request(peer_id, file_request);

                info!(%peer_id, %hash, %request_id, offset, length, "sending file request to peer");

                self.file_get_requests.insert(request_id, sender);

                let tmp_index_file = tmp_index_file.clone();
                let hash = hash.clone();
                futs.push(tokio::spawn(async move {
                    match receiver
                        .await
                        .tap_err(|err| error!(%err, %hash, "receive file response result failed"))?
                    {
                        Err(err) => {
                            error!(%err, %hash, "receive file response failed");

                            Err::<_, anyhow::Error>(err.into())
                        }

                        Ok(file_resp) => {
                            match file_resp.content {
                                None => return Ok(()),
                                Some(data) => {
                                    tmp_index_file.write_at_all(&data, offset).await.tap_err(
                                        |err| error!(%err, %hash, offset, "write index file data failed"),
                                    )?;

                                    info!(%hash, offset, "write index file data done");
                                }
                            }

                            Ok(())
                        }
                    }
                }));

                offset += length;

                remaining_task_number -= 1;
                // task number is full, need store sync status
                if remaining_task_number == 0 {
                    break;
                }
            }

            hash_file.syncing_offset = offset;
        }

        let index_dir = self.index_dir.to_path_buf();
        let store_dir = self.store_dir.to_path_buf();
        let handle = handle_sync_files_result(index_dir, store_dir, futs, need_sync_files);

        Ok(Some(handle))
    }

    #[instrument(err, skip(self))]
    async fn create_or_open_temp_index_file(&self, hash: &str) -> io::Result<File> {
        let mut tmp_path = self.index_dir.join(".tmp");
        match fs::create_dir(&tmp_path).await {
            Err(err) if err.kind() != ErrorKind::AlreadyExists => {
                error!(%err, ?tmp_path, "create temp dir failed");

                return Err(err);
            }

            Err(_) | Ok(_) => {}
        }

        tmp_path.push(hash);

        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&tmp_path)
            .await
            .tap_err(|err| error!(%err, ?tmp_path, "create temp file failed"))
    }

    #[instrument(err, skip(self))]
    async fn need_sync(&mut self) -> anyhow::Result<Option<HashMap<String, HashFile>>> {
        let store_dir = self.store_dir;

        let store_filenames: HashSet<(String, String)> = collect_filenames(store_dir)
            .await?
            .into_stream()
            .then(|filename| async move {
                let file_path = store_dir.join(&filename);
                let index_file_path = fs::read_link(&file_path)
                    .await
                    .tap_err(|err| error!(%err, ?file_path, "read symlink failed"))?;
                let hash = index_file_path.file_name().ok_or_else(|| {
                    error!(?index_file_path, "index file path doesn't have filename");

                    Error::new(
                        ErrorKind::InvalidData,
                        format!("index file path {index_file_path:?} doesn't have filename"),
                    )
                })?;

                Ok::<_, Error>((
                    filename.to_string_lossy().to_string(),
                    hash.to_string_lossy().to_string(),
                ))
            })
            .try_collect::<HashSet<_>>()
            .await?;

        info!(?store_filenames, "collect store filenames done");

        let mut hash_files = HashMap::<_, HashFile>::with_capacity(self.peer_stores.len());
        for (peer, peer_store) in self.peer_stores {
            for (filename, hash_ref) in &peer_store.files {
                let filename_hash = (filename.clone(), hash_ref.clone());
                if store_filenames.contains(&filename_hash) {
                    continue;
                }
                let (filename, hash) = filename_hash;

                hash_files
                    .entry(hash)
                    .and_modify(|hash_file| {
                        hash_file.peers.push(*peer);
                        hash_file.filenames.push(filename.clone());
                    })
                    .or_insert_with(move || HashFile {
                        hash: hash_ref.clone(),
                        filenames: vec![filename],
                        peers: vec![*peer],
                        size: peer_store.index.get(hash_ref).copied().unwrap(),
                        syncing_offset: 0,
                    });
            }
        }

        if hash_files.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hash_files))
        }
    }
}

#[derive(Debug)]
pub struct HashFile {
    hash: String,
    filenames: Vec<String>,
    peers: Vec<PeerId>,
    size: u64,
    syncing_offset: u64,
}

fn handle_sync_files_result(
    index_dir: PathBuf,
    store_dir: PathBuf,
    futs: Vec<JoinHandle<anyhow::Result<()>>>,
    mut need_sync_files: HashMap<String, HashFile>,
) -> SyncFileTask {
    tokio::spawn(async move {
        for fut in futs {
            fut.await.unwrap()?;
        }

        let tmp_dir = index_dir.join(".tmp");

        let mut finish_hash_list = vec![];
        for hash_file in need_sync_files.values() {
            // not yet finish sync
            if hash_file.syncing_offset < hash_file.size {
                continue;
            }

            info!(
                ?hash_file,
                "hash file is sync done, start move to index store and create symlink"
            );

            finish_hash_list.push(hash_file.hash.clone());

            for filename in &hash_file.filenames {
                let store_file_path = store_dir.join(filename);
                let index_file_path = index_dir.join(&hash_file.hash);
                let tmp_file_path = tmp_dir.join(&hash_file.hash);

                match fs::rename(&tmp_file_path, &index_file_path).await {
                    Err(err) if err.kind() != ErrorKind::NotFound => {
                        error!(%err, ?tmp_file_path, ?index_file_path, "move temp file to index dir failed");

                        return Err(err.into());
                    }

                    Err(_) => {}

                    Ok(_) => {
                        info!(
                            ?tmp_file_path,
                            ?index_file_path,
                            "move temp file to index dir done"
                        );
                    }
                }

                fs::symlink(&index_file_path, &store_file_path)
                    .await
                    .tap_err(|err| {
                        error!(
                            %err, ?index_file_path, ?store_file_path,
                            "create symlink failed"
                        );
                    })?;

                info!(?index_file_path, ?store_file_path, "create symlink done");
            }

            info!(
                ?hash_file,
                "hash file move to index store and create symlink"
            );
        }

        for hash in finish_hash_list {
            need_sync_files.remove(&hash);
        }

        if need_sync_files.is_empty() {
            Ok(None)
        } else {
            Ok::<_, anyhow::Error>(Some(need_sync_files))
        }
    })
}
