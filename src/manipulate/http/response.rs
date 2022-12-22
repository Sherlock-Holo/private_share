use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub files: Vec<ListFile>,
}

#[derive(Debug, Serialize)]
pub struct ListFile {
    pub filename: String,
    pub hash: String,
    pub downloaded: bool,
    pub peers: Vec<String>,
    pub size: String,
}

#[derive(Debug, Deserialize)]
pub struct AddFileRequest {
    pub file_path: String,
}
