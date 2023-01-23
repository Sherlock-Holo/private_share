use std::net::SocketAddr;

use axum::extract::connect_info::Connected;
use tokio::net::TcpStream;

#[derive(Debug, Clone)]
pub struct SocketAddrPeer {
    pub local: Option<SocketAddr>,
    pub remote: Option<SocketAddr>,
}

impl Connected<&TcpStream> for SocketAddrPeer {
    fn connect_info(target: &TcpStream) -> Self {
        Self {
            local: target.local_addr().ok(),
            remote: target.peer_addr().ok(),
        }
    }
}
