use std::path::Path;

use async_trait::async_trait;
use tower_http::services::ServeFile;

use crate::command;

#[derive(Debug, Default)]
pub struct FileGetter;

#[async_trait]
impl command::FileGetter for FileGetter {
    type FileContent = ServeFile;

    async fn get_file(self, path: &Path) -> std::io::Result<Self::FileContent> {
        Ok(ServeFile::new(path))
    }
}
