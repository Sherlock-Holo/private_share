use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub index_dir: String,
    pub store_dir: String,
}
