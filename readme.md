# Bitcoin Network Explorer

A Rust application that crawls the Bitcoin P2P network to discover unique peer nodes. By connecting to an initial peer and gathering further node information, it efficiently compiles a network map of active Bitcoin nodes.

## Features

- Connects to the Bitcoin network via the [Bitcoin P2P protocol](https://en.bitcoin.it/wiki/Protocol_documentation).
- Discovers unique peers in parallel, with configurable concurrency.
- Logs each step of the process using `tracing`, enabling insight into connection attempts, peers found, and task completion.
- Simple, extensible design for further network analysis or integration with other Bitcoin applications.

## Prerequisites

- [Rust and Cargo](https://www.rust-lang.org/tools/install)
- Bitcoin nodes reachable from the host machine

## Getting Started

### 1. Clone the repository
```bash
git clone https://github.com/thsioutas/bitcoin-network-explorer
cd bitcoin-network-explorer
```
### 2. Build the project
```
cargo build --release
```
### 3. . Run the Application
```
cargo run --release -- --initial-peer <PEER_IP:PORT>
```
Example
```
cargo run --release -- --initial-peer 47.243.121.223:8333 --connection-timeout-ms 500 --target-discovered-peers 5000 --max-concurrent-tasks 10
```
### Command-Line Options

| Option                      | Description                                                                      | Default               |
|-----------------------------|----------------------------------------------------------------------------------|-----------------------|
| `-i, --initial-peer`            | Initial peer node to connect to (e.g., via `dig seed.bitcoin.sipa.be`).          | `47.243.121.223:8333` |
| `-c, --connection-timeout-ms`   | Timeout (in milliseconds) for connecting to each peer node.                      | `500`                 |
| `-t, --target-discovered-peers` | Number of unique peers to discover before stopping.                              | `5000`                |
| `-m, --max-concurrent-tasks`    | Maximum number of concurrent peer discovery tasks allowed.                       | `10`                  |

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
