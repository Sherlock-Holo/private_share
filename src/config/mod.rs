use std::borrow::Cow;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::{io, mem};

use serde::{Deserialize, Serialize};
use tap::TapFallible;
use tokio::fs;
use tracing::{error, info, instrument};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub index_dir: String,
    pub store_dir: String,
    pub secret_key_path: String,
    pub public_key_path: String,
    pub pre_share_key: String,
    pub refresh_interval: String,
    pub sync_file_interval: String,
    pub peer_addrs: Vec<String>,
    pub http_listen: Vec<SocketAddr>,
    pub swarm_listen: String,
}

#[derive(Debug)]
pub struct ConfigManager {
    config_dir: PathBuf,
    config: Cow<'static, Config>,
}

impl ConfigManager {
    const CONFIG_FILENAME: &'static str = "config.yaml";
    const CONFIG_TMP_FILENAME: &'static str = "config.yaml.tmp";

    pub async fn new(config_dir: PathBuf) -> io::Result<Self> {
        let cfg = fs::read(config_dir.join(Self::CONFIG_FILENAME)).await?;
        let config = serde_yaml::from_slice::<Config>(&cfg)
            .map_err(|err| Error::new(ErrorKind::Other, err))?;

        Ok(Self {
            config_dir,
            config: Cow::Owned(config),
        })
    }

    #[instrument]
    pub fn load(&self) -> Cow<Config> {
        Cow::Borrowed(&self.config)
    }

    #[instrument(err)]
    pub async fn swap(&mut self, config: Cow<'_, Config>) -> io::Result<Cow<Config>> {
        let cfg_data = serde_yaml::to_string(&config).map_err(|err| {
            error!(%err, ?config, "marshal config failed");

            Error::new(ErrorKind::Other, err)
        })?;

        info!(?config, "marshal config done");

        let cfg_tmp_path = self.config_dir.join(Self::CONFIG_TMP_FILENAME);

        fs::write(&cfg_tmp_path, &cfg_data).await.tap_err(|err| {
            error!(%err, ?cfg_tmp_path, %cfg_data, "write config failed");
        })?;

        info!(?cfg_tmp_path, %cfg_data, "write config done");
        let cfg_path = self.config_dir.join(Self::CONFIG_FILENAME);

        fs::rename(&cfg_tmp_path, &cfg_path).await.tap_err(|err| {
            error!(
                %err,
                ?cfg_tmp_path,
                ?cfg_path,
                "move temp config file to real config file failed"
            )
        })?;

        info!(
            ?cfg_tmp_path,
            ?cfg_path,
            "move temp config file to real config file done"
        );

        Ok(mem::replace(
            &mut self.config,
            Cow::Owned(config.into_owned()),
        ))
    }
}
