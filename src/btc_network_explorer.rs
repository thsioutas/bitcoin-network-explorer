use crate::codec::BitcoinCodec;
use bitcoin::p2p::message::{NetworkMessage, RawNetworkMessage};
use bitcoin::p2p::message_network::VersionMessage;
use bitcoin::p2p::{Address, ServiceFlags};
use bitcoin::Network;
use futures::{SinkExt, StreamExt, TryFutureExt};
use rand::Rng;
use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::{error::Elapsed, timeout};
use tokio_util::codec::Framed;
use tracing::{debug, error, info, warn};

pub struct BtcNetworkExplorer {
    initial_address: SocketAddr,
    connection_timeout: u64,
    target_discovered_peers: usize,
    max_concurrent_tasks: usize,
    discovered_peers: Arc<Mutex<HashSet<SocketAddr>>>,
}

impl BtcNetworkExplorer {
    pub fn new(
        initial_address: SocketAddr,
        connection_timeout: u64,
        target_discovered_peers: usize,
        max_concurrent_tasks: usize,
    ) -> Self {
        Self {
            initial_address,
            connection_timeout,
            target_discovered_peers,
            max_concurrent_tasks,
            discovered_peers: Default::default(),
        }
    }

    pub async fn crawl_network(&self) {
        // How many times each peer should be processed
        const MAX_PROCESS_PEER_ATTEMPTS: u8 = 2;

        // This could also be a HashSet but due to the logic we have to add a peer to the discovered_peers
        // it can safely be a Vec
        let queued_peers = Arc::new(Mutex::new(vec![self.initial_address]));
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent_tasks));

        let mut tasks: Vec<JoinHandle<()>> = Vec::new();
        // Craw network until:
        // 1. Target number of peers are discovered, and
        // 2. The queued peers list is not empty or there is a task that is not finished (and can introduce more peers to be checked)
        while self.discovered_peers.lock().await.len() < self.target_discovered_peers
            && (!queued_peers.lock().await.is_empty()
                || tasks.iter().any(|handle| !handle.is_finished()))
        {
            while let Some(peer) = queued_peers.lock().await.pop() {
                let discovered_peers_clone = self.discovered_peers.clone();
                let connection_timeout_clone = self.connection_timeout;
                let target_discovered_peers_clone = self.target_discovered_peers;
                let queued_peers_clone = queued_peers.clone();
                let semaphore = semaphore.clone();

                tasks.push(tokio::spawn(async move {
                    let _permit = semaphore.acquire().await;
                    // Check again if we need to process the peer. The target could have been reached while waiting for semaphore's permit.
                    if discovered_peers_clone.lock().await.len() >= target_discovered_peers_clone {
                        return;
                    }
                    if let Err(e) = process_peer_with_retry(
                        peer,
                        MAX_PROCESS_PEER_ATTEMPTS,
                        discovered_peers_clone,
                        queued_peers_clone,
                        connection_timeout_clone,
                        target_discovered_peers_clone,
                    )
                    .await
                    {
                        error!("Failed to process peer {} after retries: {}", peer, e);
                    }
                }));
            }
        }

        // Wait for all tasks to finish
        futures::future::join_all(tasks).await;
    }

    pub async fn get_discovered_peers_num(&self) -> usize {
        self.discovered_peers.lock().await.len()
    }
}

/// Process a peer with retry logic
async fn process_peer_with_retry(
    peer: SocketAddr,
    max_attempts: u8,
    discovered_peers: Arc<Mutex<HashSet<SocketAddr>>>,
    queued_peers: Arc<Mutex<Vec<SocketAddr>>>,
    connection_timeout: u64,
    target_discovered_peers: usize,
) -> Result<(), Error> {
    let mut attempts = 0;

    while attempts < max_attempts {
        attempts += 1;

        match process_peer(
            peer,
            connection_timeout,
            target_discovered_peers,
            discovered_peers.clone(),
            queued_peers.clone(),
        )
        .await
        {
            Ok(_) => {
                let discovered_count = discovered_peers.lock().await.len();
                let remaining_peers = target_discovered_peers.saturating_sub(discovered_count);
                let queued_count = queued_peers.lock().await.len();
                info!("Processed peer {} on attempt {}/{}. Still need to discover {} peers. {} peers in queue to be checked.", 
                peer, attempts, max_attempts, remaining_peers, queued_count);
                return Ok(());
            }
            Err(e) => {
                debug!(
                    "Failed to process peer {} on attempt {}/{}: {}",
                    peer, attempts, max_attempts, e
                );
                if attempts == max_attempts {
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

/// Process a single peer by:
/// 1. performing the necessary handshake, and
/// 2. collect new peer addresses
async fn process_peer(
    peer: SocketAddr,
    connection_timeout: u64,
    target_discovered_peers: usize,
    discovered_peers: Arc<Mutex<HashSet<SocketAddr>>>,
    queued_peers: Arc<Mutex<Vec<SocketAddr>>>,
) -> Result<(), Error> {
    // Connect and perform handshake
    let mut stream = connect(&peer, connection_timeout).await?;
    perform_handshake(&mut stream, peer).await?;

    // Collect new peers from the given peer
    let new_peers = collect_peers(&mut stream).await?;

    // Add new peers to the list of the unique discovered peers
    // Also, add these peers to the queued_peers list
    for peer in new_peers {
        if discovered_peers.lock().await.len() >= target_discovered_peers {
            break;
        } else if discovered_peers.lock().await.insert(peer) {
            queued_peers.lock().await.push(peer);
        }
    }

    Ok(())
}

/// Collect peers from the given stream using the Bitcoin P2P protocol.
///
/// Listens for `Addr` messages and parses peer information.
///
/// # Parameters
/// - `stream`: The TCP stream connected to the peer.
/// # Returns
/// A list of new peers discovered from the `Addr` message.
async fn collect_peers(
    stream: &mut Framed<TcpStream, BitcoinCodec>,
) -> Result<Vec<SocketAddr>, Error> {
    let collect_peers_task = async {
        let mut new_peers = Vec::new();
        while let Some(result) = stream.next().await {
            match result {
                Ok(message) => match message.payload() {
                    NetworkMessage::Addr(addr_list) => {
                        for addr in addr_list {
                            if let Ok(socket_addr) = addr.1.socket_addr() {
                                if socket_addr.is_ipv4() {
                                    new_peers.push(socket_addr);
                                }
                            }
                        }
                        break;
                    }
                    NetworkMessage::Version(_) | NetworkMessage::Verack => {
                        stream
                            .send(RawNetworkMessage::new(
                                Network::Bitcoin.magic(),
                                NetworkMessage::GetAddr,
                            ))
                            .await
                            .map_err(Error::SendingFailed)?;
                    }
                    NetworkMessage::Alert(_) | NetworkMessage::Ping(_) => {
                        debug!("Ignoring whitelisted messages");
                    }
                    _ => warn!("Ignoring unsupported message type"),
                },
                Err(e) => {
                    warn!("Error processing message: {}", e);
                    return Err(Error::ConnectionLost);
                }
            }
        }
        Ok(new_peers)
    };
    let collect_peers_timeout = Duration::from_secs(2);
    timeout(collect_peers_timeout, collect_peers_task)
        .map_err(|_| Error::CollectPeersTimeout)
        .await?
}

async fn connect(
    remote_address: &SocketAddr,
    connection_timeout: u64,
) -> Result<Framed<TcpStream, BitcoinCodec>, Error> {
    let connection = TcpStream::connect(remote_address).map_err(Error::ConnectionFailed);
    let stream = timeout(Duration::from_millis(connection_timeout), connection)
        .map_err(Error::ConnectionTimedOut)
        .await??;
    let framed = Framed::new(stream, BitcoinCodec {});
    Ok(framed)
}

/// Perform a Bitcoin handshake as per [this protocol documentation](https://en.bitcoin.it/wiki/Protocol_documentation)
async fn perform_handshake(
    stream: &mut Framed<TcpStream, BitcoinCodec>,
    peer: SocketAddr,
) -> Result<(), Error> {
    let version_message = RawNetworkMessage::new(
        Network::Bitcoin.magic(),
        NetworkMessage::Version(build_version_message(&peer)),
    );

    stream
        .send(version_message)
        .await
        .map_err(Error::SendingFailed)?;
    let handshake_task = async {
        while let Some(result) = stream.next().await {
            match result {
                Ok(message) => match message.payload() {
                    NetworkMessage::Version(remote_version) => {
                        debug!("Version message: {:?}", remote_version);
                        stream
                            .send(RawNetworkMessage::new(
                                Network::Bitcoin.magic(),
                                NetworkMessage::Verack,
                            ))
                            .await
                            .map_err(Error::SendingFailed)?;

                        return Ok(());
                    }
                    other_message => {
                        // We're only interested in the version message. Keep the loop running.
                        debug!("Unsupported message: {:?}", other_message);
                    }
                },
                Err(err) => {
                    error!("Decoding error: {}", err);
                }
            }
        }
        Err(Error::ConnectionLost)
    };
    let handshake_timeout = Duration::from_secs(1);
    timeout(handshake_timeout, handshake_task)
        .map_err(|_| Error::HandshakeTimeout)
        .await?
}

fn build_version_message(receiver_address: &SocketAddr) -> VersionMessage {
    // The height of the block that the node is currently at.
    // We are always at the genesis block. because our implementation is not a real node.
    const START_HEIGHT: i32 = 0;
    // The most popular user agent. See https://bitnodes.io/nodes/
    const USER_AGENT: &str = "/Satoshi:25.0.0/";
    const SERVICES: ServiceFlags = ServiceFlags::NONE;
    // The address of this local node.
    // This address doesn't matter much as it will be ignored by the bitcoind node in most cases.
    let sender_address: SocketAddr =
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0));

    let sender = Address::new(&sender_address, SERVICES);
    let timestamp = chrono::Utc::now().timestamp();
    let receiver = Address::new(receiver_address, SERVICES);
    let nonce = rand::thread_rng().gen();
    let user_agent = USER_AGENT.to_string();

    VersionMessage::new(
        SERVICES,
        timestamp,
        receiver,
        sender,
        nonce,
        user_agent,
        START_HEIGHT,
    )
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Connection failed: {0:?}")]
    ConnectionFailed(std::io::Error),
    #[error("Connection timed out")]
    ConnectionTimedOut(Elapsed),
    #[error("Connection lost")]
    ConnectionLost,
    #[error("Sending failed")]
    SendingFailed(std::io::Error),
    #[error("Handshake timed out")]
    HandshakeTimeout,
    #[error("Collecting peers timed out")]
    CollectPeersTimeout,
}
