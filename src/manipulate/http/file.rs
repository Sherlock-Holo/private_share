use std::path::Path;

use async_trait::async_trait;
use http_dir::fs::disk::DiskFilesystem;
use http_dir::ServeFile;
use tracing::instrument;

use crate::command;

#[derive(Debug, Default)]
pub struct FileGetter;

#[async_trait]
impl command::FileGetter for FileGetter {
    type FileContent = ServeFile<DiskFilesystem>;

    #[instrument(err)]
    async fn get_file(self, path: &Path) -> std::io::Result<Self::FileContent> {
        let parent = path
            .parent()
            .unwrap_or_else(|| panic!("checked path {path:?} has no parent"));
        let filename = path
            .file_name()
            .unwrap_or_else(|| panic!("checked path {path:?} has no filename"));

        Ok(ServeFile::new(
            filename,
            DiskFilesystem::new(parent.to_path_buf()),
        ))
    }
}
