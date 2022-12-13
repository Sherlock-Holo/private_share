use std::ffi::OsString;
use std::io;
use std::io::{Error, ErrorKind};
use std::path::Path;

use futures_util::{stream, StreamExt, TryFutureExt, TryStreamExt};
use libp2p::gossipsub::Sha256Topic;
use libp2p::Swarm;
use prost::Message as _;
use tap::TapFallible;
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;
use tracing::{error, info, instrument};

use crate::node::behaviour::Behaviour;
use crate::node::message::{File, FileMessage};

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

        let store_filenames = collect_filenames(store_dir).await?;

        info!(?store_filenames, "collect store files done");

        let files = stream::iter(store_filenames.iter())
            .then(|filename| async move {
                let store_file_path = store_dir.join(filename);

                let index_file_path = fs::read_link(&store_file_path)
                    .await
                    .tap_err(|err| error!(%err, ?store_file_path, "read symlink failed"))?;

                let index_filename = index_file_path.file_name().ok_or_else(|| {
                    error!(?index_file_path, "index file doesn't contain filename");

                    Error::new(
                        ErrorKind::Other,
                        format!("index file {index_file_path:?} doesn't contain filename"),
                    )
                })?;

                Ok::<_, Error>((filename, index_filename.to_owned()))
            })
            .map_ok(|(filename, index_filename): (&OsString, OsString)| File {
                filename: filename.to_string_lossy().to_string(),
                hash: index_filename.to_string_lossy().to_string(),
            })
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

#[instrument(err)]
async fn collect_filenames(dir: &Path) -> io::Result<Vec<OsString>> {
    let read_dir = ReadDirStream::new(
        fs::read_dir(dir)
            .await
            .tap_err(|err| error!(%err, ?dir, "read dir failed"))?,
    );

    read_dir
        .try_filter_map(|entry| async move {
            let metadata = entry
                .metadata()
                .await
                .tap_err(|err| error!(%err, ?dir, "get entry metadata failed"))?;
            Ok(metadata.is_file().then_some(entry))
        })
        .map_ok(|entry| entry.file_name())
        .try_collect()
        .await
}
