use std::ffi::OsString;
use std::io;
use std::path::Path;

use futures_util::TryStreamExt;
use tap::TapFallible;
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;
use tracing::{error, instrument};

#[instrument(err)]
pub async fn collect_filenames(dir: &Path) -> io::Result<Vec<OsString>> {
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
