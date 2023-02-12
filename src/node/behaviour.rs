use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;
use std::io::{Error, ErrorKind};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use either::Either;
use futures_util::{AsyncRead, AsyncWrite, AsyncWriteExt};
use libp2p::core::upgrade::{read_length_prefixed, write_length_prefixed};
use libp2p::core::ProtocolName;
use libp2p::gossipsub::{
    Gossipsub, GossipsubConfigBuilder, GossipsubMessage, MessageAuthenticity, MessageId,
    Sha256Topic, ValidationMode,
};
use libp2p::identity::Keypair;
use libp2p::request_response::{
    ProtocolSupport, RequestResponse, RequestResponseCodec, RequestResponseConfig,
};
use libp2p::swarm::{dummy, keep_alive, NetworkBehaviour};
use libp2p::{identify, ping};
use libp2p_auto_relay::{endpoint, relay};
use once_cell::sync::Lazy;
use prost::Message;
use tap::TapFallible;
use tracing::{error, info, instrument};

/// max 16MiB
pub const MAX_CHUNK_SIZE: usize = 16 * 1024 * 1024;
const IDENTIFY_PROTOCOL: &str = "private-share-identify/0.1.0";

pub static FILE_SHARE_TOPIC: Lazy<Sha256Topic> = Lazy::new(|| {
    const TOPIC: &str = "private-share";

    Sha256Topic::new(TOPIC)
});

pub static DISCOVER_SHARE_TOPIC: Lazy<Sha256Topic> = Lazy::new(|| {
    const TOPIC: &str = "private-share/discover";

    Sha256Topic::new(TOPIC)
});

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    pub(crate) gossip: Gossipsub,
    pub(crate) request_respond: RequestResponse<FileCodec>,
    pub(crate) keepalive: keep_alive::Behaviour,
    pub(crate) ping: ping::Behaviour,
    pub(crate) identify: identify::Behaviour,
    pub(crate) relay: Either<dummy::Behaviour, relay::Behaviour>,
    pub(crate) endpoint: Either<dummy::Behaviour, endpoint::Behaviour>,
}

impl Behaviour {
    pub fn new(
        key: Keypair,
        enable_relay_behaviour: bool,
        endpoint_behaviour: Option<endpoint::Behaviour>,
    ) -> anyhow::Result<Self> {
        let public_key = key.public();

        let gossipsub_config = GossipsubConfigBuilder::default()
            // This is set to aid debugging by not cluttering the log space
            .heartbeat_interval(Duration::from_secs(10))
            // This sets the kind of message validation. The default is Strict (enforce message signing)
            .validation_mode(ValidationMode::Strict)
            // content-address messages. No two messages of the same content will be propagated.
            .message_id_fn(create_gossip_message_id)
            .build()
            .map_err(|err| anyhow::anyhow!("{}", err))?;

        let mut gossipsub = Gossipsub::new(MessageAuthenticity::Signed(key), gossipsub_config)
            .map_err(|err| anyhow::anyhow!("{}", err))?;

        gossipsub.subscribe(&FILE_SHARE_TOPIC)?;
        gossipsub.subscribe(&DISCOVER_SHARE_TOPIC)?;

        Ok(Self {
            gossip: gossipsub,
            request_respond: RequestResponse::new(
                FileCodec,
                [(FileProtocol, ProtocolSupport::Full)],
                RequestResponseConfig::default(),
            ),
            keepalive: Default::default(),
            ping: Default::default(),
            identify: identify::Behaviour::new(identify::Config::new(
                IDENTIFY_PROTOCOL.to_string(),
                public_key,
            )),
            relay: enable_relay_behaviour
                .then(|| Either::Right(Default::default()))
                .unwrap_or(Either::Left(dummy::Behaviour {})),
            endpoint: endpoint_behaviour
                .map(Either::Right)
                .unwrap_or(Either::Left(dummy::Behaviour {})),
        })
    }
}

fn create_gossip_message_id(message: &GossipsubMessage) -> MessageId {
    let mut s = DefaultHasher::new();
    message.data.hash(&mut s);
    MessageId::from(s.finish().to_string())
}

#[derive(Clone)]
pub struct FileProtocol;

impl ProtocolName for FileProtocol {
    fn protocol_name(&self) -> &[u8] {
        "/file-share/1".as_bytes()
    }
}

#[derive(Debug, Clone)]
pub struct FileCodec;

#[async_trait]
impl RequestResponseCodec for FileCodec {
    type Protocol = FileProtocol;
    type Request = FileRequest;
    type Response = FileResponse;

    #[instrument(err, skip(self, _protocol, io))]
    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let data = read_length_prefixed(io, MAX_CHUNK_SIZE)
            .await
            .tap_err(|err| error!(%err, "read request failed"))?;
        if data.is_empty() {
            error!("empty data, unexpected EOF");

            return Err(Error::from(ErrorKind::UnexpectedEof));
        }

        info!("read request data done");

        Ok(FileRequest::decode(data.as_slice()).map_err(|err| {
            error!(%err, "decode file request failed");

            Error::new(ErrorKind::Other, err)
        })?)
    }

    #[instrument(err, skip(self, _protocol, io))]
    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let data = read_length_prefixed(io, MAX_CHUNK_SIZE)
            .await
            .tap_err(|err| error!(%err, "read response failed"))?;
        if data.is_empty() {
            error!("empty data, unexpected EOF");

            return Err(Error::from(ErrorKind::UnexpectedEof));
        }

        info!("read response data done");

        Ok(FileResponse::decode(data.as_slice()).map_err(|err| {
            error!(%err, "decode file response failed");

            Error::new(ErrorKind::Other, err)
        })?)
    }

    #[instrument(err, skip(self, _protocol, io, req))]
    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let data = req.encode_to_vec();

        write_length_prefixed(io, data)
            .await
            .tap_err(|err| error!(%err, "write file request failed"))?;

        info!("write file request done");

        io.close().await?;

        Ok(())
    }

    #[instrument(err, skip(self, _protocol, io, resp))]
    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        resp: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let data = resp.encode_to_vec();

        write_length_prefixed(io, data)
            .await
            .tap_err(|err| error!(%err, "write file request failed"))?;

        info!("write file response done");

        io.close().await?;

        Ok(())
    }
}

#[derive(Message, Clone)]
pub struct FileRequest {
    #[prost(string, tag = "1")]
    pub filename: String,

    #[prost(string, tag = "2")]
    pub hash: String,

    #[prost(uint64, tag = "3")]
    pub offset: u64,

    #[prost(uint64, tag = "4")]
    pub length: u64,
}

#[derive(Message, Clone)]
pub struct FileResponse {
    #[prost(bytes = "bytes", optional, tag = "1")]
    pub content: Option<Bytes>,
}
