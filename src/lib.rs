#![feature(type_alias_impl_trait)]

use std::io;
use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use clap::Parser;
use futures_channel::mpsc;
use futures_util::stream;
use itertools::Itertools;
use libp2p::pnet::PreSharedKey;
use libp2p::Multiaddr;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio_util::time::DelayQueue;
use tracing::level_filters::LevelFilter;
use tracing::{debug, subscriber};
use tracing_log::LogTracer;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, Registry};

use crate::args::{Cli, Mode};
use crate::config::ConfigManager;
use crate::manipulate::http::{MultiAddrListener, Server};
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
    let cli = Cli::parse();
    let args = match cli.mode {
        Mode::GenPeerId {
            secret_key_path,
            public_key_path,
        } => {
            let keypair =
                load_keypair(Path::new(&secret_key_path), Path::new(&public_key_path)).await?;

            println!("{}", keypair.public().to_peer_id());

            return Ok(());
        }

        Mode::Run(args) => args,
    };

    init_log(args.debug);

    let config_manager = ConfigManager::new(args.config_dir.into()).await?;
    let config = config_manager.load();
    let swarm_addr = config.swarm_listen.parse::<Multiaddr>()?;
    let relay_server_addr = config
        .relay_server_addr
        .as_ref()
        .map(|addr| addr.parse::<Multiaddr>())
        .transpose()?;
    let keypair = load_keypair(
        Path::new(&config.secret_key_path),
        Path::new(&config.public_key_path),
    )
    .await?;

    let peer_addrs = config
        .peer_addrs
        .iter()
        .map(|addr| addr.parse::<Multiaddr>())
        .try_collect::<_, Vec<_>, _>()?;

    pre_create_dir(Path::new(&config.store_dir), Path::new(&config.index_dir)).await?;

    debug!(store_dir = %config.store_dir, index_dir = %config.index_dir, "pre create dir done");

    let mut addr_queue = DelayQueue::with_capacity(peer_addrs.len());
    for peer_addr in peer_addrs {
        addr_queue.insert(peer_addr, Duration::from_secs(0));
    }

    let mut hasher = Sha256::new();
    hasher.update(config.pre_share_key.as_bytes());
    let pre_shared_key = PreSharedKey::new(hasher.finalize().into());

    let node_config = NodeConfig {
        key: keypair,
        index_dir: config.index_dir.clone().into(),
        store_dir: config.store_dir.clone().into(),
        handshake_key: pre_shared_key,
        refresh_store_interval: humantime::parse_duration(&config.refresh_interval)?,
        sync_file_interval: humantime::parse_duration(&config.sync_file_interval)?,
        enable_relay_behaviour: args.enable_relay_service,
        relay_server_addr,
    };

    let (command_sender, command_receiver) = mpsc::channel(1);

    let multi_addr_listener =
        MultiAddrListener::new(stream::iter(config.http_listen.iter().copied())).await?;
    let mut node = Node::new(node_config, addr_queue, command_receiver, config_manager)?;
    let http_server = Server::new(command_sender);

    tokio::spawn(async move { http_server.listen(multi_addr_listener).await });

    node.run(swarm_addr).await
}

async fn pre_create_dir(store_dir: &Path, index_dir: &Path) -> io::Result<()> {
    if let Err(err) = fs::create_dir_all(store_dir).await {
        if err.kind() != ErrorKind::AlreadyExists {
            return Err(err);
        }
    }
    if let Err(err) = fs::create_dir_all(index_dir).await {
        if err.kind() != ErrorKind::AlreadyExists {
            return Err(err);
        }
    }

    Ok(())
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
