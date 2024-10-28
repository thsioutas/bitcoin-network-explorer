mod btc_network_explorer;
mod codec;
mod tracing_helper;

use btc_network_explorer::BtcNetworkExplorer;
use clap::Parser;
use std::net::SocketAddr;
use tracing::info;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The address of the inital node to reach to.
    /// `dig seed.bitcoin.sipa.be +short` may provide a fresh list of nodes.
    #[arg(short, long, default_value = "47.243.121.223:8333")]
    initial_peer: String,

    /// The connection timeout, in milliseconds.
    /// Most nodes will quickly respond. If they don't, we'll probably want to talk to other nodes instead.
    #[arg(short, long, default_value = "500")]
    connection_timeout_ms: u64,

    /// The target number of unique peers to collect.
    #[arg(short, long, default_value = "5000")]
    target_discovered_peers: usize,

    /// The max number of concurrent tasks that are allowed to crawl in parallel.
    #[arg(short, long, default_value = "10")]
    max_concurrent_tasks: usize,
}

#[tokio::main]
async fn main() {
    tracing_helper::init_tracing();
    let args = Args::parse();

    // Parse the provided initial address
    let initial_peer = args
        .initial_peer
        .parse::<SocketAddr>()
        .expect("Invalid remote address");

    // Start network crawling
    info!("Start network crawling");
    let btc_network_explorer = BtcNetworkExplorer::new(
        initial_peer,
        args.connection_timeout_ms,
        args.target_discovered_peers,
        args.max_concurrent_tasks,
    );
    let start = std::time::Instant::now();
    btc_network_explorer.crawl_network().await;
    let elapsed = start.elapsed().as_secs();
    info!(
        "Discovered {} peers in {}secs",
        btc_network_explorer.get_discovered_peers_num().await,
        elapsed
    );
}
