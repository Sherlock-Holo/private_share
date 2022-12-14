use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub files: Vec<ListFile>,
}

#[derive(Debug, Serialize)]
pub struct ListFile {
    pub filename: String,
    pub hash: String,
}

#[derive(Debug, Deserialize)]
pub struct AddFileRequest {
    pub file_path: String,
}

pub struct AddFileResponse {
    pub hash: String,
}
