use std::ffi::OsString;
use std::io;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use ed25519::pkcs8::{DecodePrivateKey, DecodePublicKey, PublicKeyBytes};
use ed25519::KeypairBytes;
use futures_util::TryStreamExt;
use libp2p::identity;
use libp2p::identity::Keypair;
use tap::TapFallible;
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;
use tracing::{error, info, instrument};

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
            Ok((!metadata.is_dir()).then_some(entry.file_name()))
        })
        .try_collect()
        .await
}

#[instrument(err)]
pub async fn create_temp_dir(path: &Path) -> io::Result<PathBuf> {
    let tmp_path = path.join(".tmp");

    match fs::create_dir_all(&tmp_path).await {
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            info!(?path, ?tmp_path, "temp dir exists");

            Ok(tmp_path)
        }

        Err(err) => {
            error!(%err, ?path, ?tmp_path, "create temp die failed");

            Err(err)
        }

        Ok(_) => {
            info!(?path, ?tmp_path, "create temp dir done");

            Ok(tmp_path)
        }
    }
}

pub async fn load_keypair(secret_path: &Path, public_path: &Path) -> anyhow::Result<Keypair> {
    let secret = fs::read_to_string(secret_path).await?;
    let mut keypair = KeypairBytes::from_pkcs8_pem(&secret)?;
    let public = fs::read_to_string(public_path).await?;
    let public = PublicKeyBytes::from_public_key_pem(&public)?;

    keypair.public_key.replace(public.to_bytes());

    let keypair = identity::ed25519::Keypair::decode(&mut keypair.to_bytes().unwrap())?;
    Ok(Keypair::Ed25519(keypair))
}

#[cfg(test)]
mod tests {
    use libp2p::PeerId;

    use super::*;

    #[tokio::test]
    async fn test_load_keypair() {
        let keypair = load_keypair(Path::new("secret.pem"), Path::new("public.pem"))
            .await
            .unwrap();

        let peer_id = "12D3KooWD3q89dKJEK6rLi5VnzCVkJETdnnGok8XfefnhET723tY"
            .parse::<PeerId>()
            .unwrap();

        assert_eq!(peer_id, keypair.public().to_peer_id());
    }
}
