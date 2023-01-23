use std::io;
use std::io::Error;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{stream, Stream, StreamExt, TryStreamExt};
use hyper::server::accept::Accept;
use tap::TapFallible;
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::wrappers::TcpListenerStream;
use tracing::error;

#[derive(Debug)]
pub struct MultiAddrListener {
    listeners: Vec<TcpListenerStream>,
}

impl MultiAddrListener {
    pub async fn new<S: Stream<Item = SocketAddr>>(addrs: S) -> io::Result<Self> {
        let st = addrs.then(|addr| async move {
            let listener = TcpListener::bind(addr)
                .await
                .tap_err(|err| error!(%err, %addr, "bind tcp listener failed"))?;

            Ok::<_, Error>(TcpListenerStream::new(listener))
        });

        let listeners = st
            .try_collect()
            .await
            .tap_err(|err| error!(%err, "collect tcp listeners failed"))?;

        Ok(Self { listeners })
    }
}

impl Accept for MultiAddrListener {
    type Conn = TcpStream;
    type Error = Error;

    fn poll_accept(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        stream::select_all(self.listeners.iter_mut()).poll_next_unpin(cx)
    }
}
