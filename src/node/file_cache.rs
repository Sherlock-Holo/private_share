use std::future::Future;
use std::io;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use lru::LruCache;
use tokio::fs::File;
use tracing::instrument;

// Safety: 64 > 0
const MAX_FILE_COUNT: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(64) };

#[derive(Debug)]
pub struct FileCache {
    files: LruCache<String, (Arc<File>, Instant)>,
}

impl FileCache {
    pub fn new() -> Self {
        Self {
            files: LruCache::new(MAX_FILE_COUNT),
        }
    }

    #[instrument(err, skip(open_fn))]
    pub async fn get_or_open_file<Fut: Future<Output = io::Result<File>>, F: FnOnce() -> Fut>(
        &mut self,
        hash: &str,
        open_fn: F,
    ) -> io::Result<Arc<File>> {
        if let Some((file, instant)) = self.files.get_mut(hash) {
            *instant = Instant::now();

            return Ok(file.clone());
        }

        let file = open_fn().await?;
        let file = Arc::new(file);
        let instant = Instant::now();

        self.files.put(hash.to_string(), (file.clone(), instant));

        Ok(file)
    }

    pub fn clean_timeout(&mut self, timeout: Duration) {
        let timeout_hash_list = self
            .files
            .iter()
            .filter_map(|(hash, (_, instant))| {
                (instant.elapsed() > timeout).then(|| hash.to_owned())
            })
            .collect::<Vec<_>>();

        for hash in timeout_hash_list {
            self.files.pop(&hash);
        }
    }
}
