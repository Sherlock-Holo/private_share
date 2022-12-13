use std::io;
use std::io::{Error, ErrorKind};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{AsyncRead, AsyncWrite, AsyncWriteExt};
use libp2p::core::upgrade::{read_length_prefixed, write_length_prefixed};
use libp2p::core::ProtocolName;
use libp2p::gossipsub::Gossipsub;
use libp2p::request_response::{RequestResponse, RequestResponseCodec};
use libp2p::swarm::NetworkBehaviour;
use prost::Message;
use tap::TapFallible;
use tracing::{error, info, instrument};

// max 100MiB
const MAX_CHUNK_SIZE: usize = 100 * 1024;

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    pub(crate) gossip: Gossipsub,
    pub(crate) request_respond: RequestResponse<FileCodec>,
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
