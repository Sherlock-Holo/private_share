use std::io;
use std::path::Path;

use async_trait::async_trait;

#[async_trait]
pub trait FileGetter {
    type FileContent: Send + 'static;

    async fn get_file(self, path: &Path) -> io::Result<Self::FileContent>;
}
