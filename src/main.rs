mod btc_explorer;
mod codec;
mod tracing_helper;

use clap::Parser;
use std::net::SocketAddr;
use tracing::info;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The address of the node to reach to.
    /// `dig seed.bitcoin.sipa.be +short` may provide a fresh list of nodes.
    #[arg(short, long, default_value = "47.243.121.223:8333")]
    remote_address: String,

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

    // Parse the provided remote address
    // To be used as the initial peer address
    let remote_address = args
        .remote_address
        .parse::<SocketAddr>()
        .expect("Invalid remote address");

    // Start network crawling
    info!("Start network crawling");
    let start = std::time::Instant::now();
    let addresses = btc_explorer::crawl_network(
        remote_address,
        args.connection_timeout_ms,
        args.target_discovered_peers,
        args.max_concurrent_tasks,
    )
    .await;
    let elapsed = start.elapsed().as_secs();
    info!("Discovered {} peers in {}secs", addresses.len(), elapsed);
}
