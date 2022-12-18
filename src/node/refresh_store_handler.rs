use std::ffi::OsString;
use std::io::{Error, ErrorKind};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use futures_util::{stream, StreamExt, TryStreamExt};
use libp2p::gossipsub::Sha256Topic;
use libp2p::Swarm;
use prost::Message as _;
use tap::TapFallible;
use tokio::fs;
use tracing::{error, info, instrument};

use crate::node::behaviour::Behaviour;
use crate::node::message::{File, FileMessage};
use crate::util;

pub struct RefreshStoreHandler<'a> {
    store_dir: &'a Path,
    swarm: &'a mut Swarm<Behaviour>,
}

impl<'a> RefreshStoreHandler<'a> {
    pub fn new(store_dir: &'a Path, swarm: &'a mut Swarm<Behaviour>) -> Self {
        Self { store_dir, swarm }
    }

    #[instrument(err, skip(self))]
    pub async fn handle_tick(self, topic: Sha256Topic) -> anyhow::Result<()> {
        let store_dir = self.store_dir;

        let store_filenames = util::collect_filenames(store_dir).await?;

        info!(?store_filenames, "collect store files done");

        let files = stream::iter(store_filenames.iter())
            .then(|filename| async move {
                let store_file_path = store_dir.join(filename);

                let index_file_path = fs::read_link(&store_file_path)
                    .await
                    .tap_err(|err| error!(%err, ?store_file_path, "read symlink failed"))?;

                let metadata = fs::metadata(&index_file_path)
                    .await
                    .tap_err(|err| error!(%err, ?index_file_path, "get metadata failed"))?;
                let file_size = metadata.size();

                let index_filename = index_file_path.file_name().ok_or_else(|| {
                    error!(?index_file_path, "index file doesn't contain filename");

                    Error::new(
                        ErrorKind::Other,
                        format!("index file {index_file_path:?} doesn't contain filename"),
                    )
                })?;

                Ok::<_, Error>((filename, index_filename.to_owned(), file_size))
            })
            .map_ok(
                |(filename, index_filename, file_size): (&OsString, OsString, u64)| File {
                    filename: filename.to_string_lossy().to_string(),
                    hash: index_filename.to_string_lossy().to_string(),
                    file_size,
                },
            )
            .try_collect::<Vec<_>>()
            .await?;

        info!(?files, "collect message files hash done");

        let message = FileMessage {
            peer_id: self.swarm.local_peer_id().to_base58(),
            file_list: files,
        };
        let message = message.encode_to_vec();

        self.swarm
            .behaviour_mut()
            .gossip
            .publish(topic.clone(), message)
            .tap_err(|err| error!(%err, ?topic, "publish message to topic failed"))?;

        info!(?topic, "publish message to topic done");

        Ok(())
    }
}
