use std::io;
use std::path::Path;

use clap::Parser;
use futures_channel::mpsc;
use futures_util::{stream, SinkExt, StreamExt};
use itertools::Itertools;
use libp2p::pnet::PreSharedKey;
use libp2p::Multiaddr;
use sha2::{Digest, Sha256};
use tokio::fs;
use tracing::level_filters::LevelFilter;
use tracing::subscriber;
use tracing_log::LogTracer;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, Registry};

use crate::args::Args;
use crate::config::Config;
use crate::manipulate::http::Server;
use crate::node::config::Config as NodeConfig;
use crate::node::Node;
use crate::util::load_keypair;

mod args;
mod command;
mod config;
mod ext;
mod manipulate;
mod node;
mod util;

pub async fn run() -> anyhow::Result<()> {
    let args = Args::parse();

    init_log(args.debug);

    let config = fs::read(args.config).await?;
    let config = serde_yaml::from_slice::<Config>(&config)?;
    let keypair = load_keypair(
        Path::new(&config.secret_key_path),
        Path::new(&config.public_key_path),
    )
    .await?;

    let peer_addrs = config
        .peer_addrs
        .into_iter()
        .map(|addr| addr.parse::<Multiaddr>())
        .try_collect::<_, Vec<_>, _>()?;
    let (mut addr_sender, addr_receiver) = mpsc::channel(peer_addrs.len());
    addr_sender
        .send_all(&mut stream::iter(peer_addrs).map(Ok))
        .await
        .unwrap();

    let mut hasher = Sha256::new();
    hasher.update(config.pre_share_key.as_bytes());
    let pre_shared_key = PreSharedKey::new(hasher.finalize().into());

    let node_config = NodeConfig {
        key: keypair,
        index_dir: config.index_dir.into(),
        store_dir: config.store_dir.into(),
        handshake_key: pre_shared_key,
        refresh_store_interval: humantime::parse_duration(&config.refresh_interval)?,
        sync_file_interval: humantime::parse_duration(&config.sync_file_interval)?,
    };

    let (command_sender, command_receiver) = mpsc::channel(1);

    let mut node = Node::new(node_config, addr_receiver, command_receiver)?;
    let http_server = Server::new(command_sender);
    let swarm_addr = config.swarm_listen.parse::<Multiaddr>()?;

    tokio::spawn(async move { http_server.listen(config.http_listen).await });

    node.run(swarm_addr).await
}

fn init_log(debug: bool) {
    LogTracer::init().unwrap();

    let layer = fmt::layer()
        .pretty()
        .with_target(true)
        .with_writer(io::stderr);

    let level = if debug {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };

    let targets = Targets::new()
        .with_target("h2", LevelFilter::OFF)
        .with_default(LevelFilter::DEBUG);

    let layered = Registry::default().with(targets).with(layer).with(level);

    subscriber::set_global_default(layered).unwrap();
}
