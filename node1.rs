// N O D E 1 with relay support (dialer)

use libp2p::{
    core::{upgrade, Multiaddr},
    identity::{self, Keypair, ed25519},
    noise,
    ping::{Behaviour as Ping, Config as PingConfig, Event as PingEvent},
    relay::client::{self, Behaviour as RelayClient},
    swarm::{SwarmBuilder, SwarmEvent, NetworkBehaviour},
    tcp, yamux,
    identify,
    PeerId, Transport,
    futures::StreamExt,
};

use std::error::Error;
use std::fs;
use std::path::Path;
use tokio;

const KEY_FILE: &str = "src/p2p/identity_key_1.pk";

fn load_or_generate_identity() -> Keypair {
    if Path::new(KEY_FILE).exists() {
        let mut bytes = fs::read(KEY_FILE).expect("Failed to read identity key file");
        let secret =
            ed25519::SecretKey::try_from_bytes(&mut bytes).expect("Invalid key bytes in file");
        let keypair = ed25519::Keypair::from(secret);
        Keypair::from(keypair)
    } else {
        let id_keys = ed25519::Keypair::generate();
        fs::write(KEY_FILE, id_keys.secret().as_ref()).expect("Failed to write identity key");
        Keypair::from(id_keys)
    }
}

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    relay: RelayClient,
    ping: Ping,
    identify: identify::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let local_key = load_or_generate_identity();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Node 1 - Peer ID: {}", local_peer_id);

    let transport = tcp::tokio::Transport::default()
        .upgrade(upgrade::Version::V1Lazy)
        .authenticate(noise::Config::new(&local_key)?)
        .multiplex(yamux::Config::default())
        .boxed();

    let behaviour = MyBehaviour {
        relay: client::Behaviour::new(local_peer_id),
        ping: Ping::new(PingConfig::new()),
        identify: identify::Behaviour::new(identify::Config::new(
            "/relay-chat/0.1.0".into(),
            local_key.public(),
        )),
    };

    let mut swarm = SwarmBuilder::with_tokio_executor()
        .with_transport(transport)
        .with_behaviour(behaviour)
        .with_peer_id(local_peer_id)
        .build();

    // Listen on your local TCP port (for local tests if needed)
    swarm.listen_on("/ip4/0.0.0.0/tcp/9000".parse()?)?;

    // Relay server address
    let relay_peer_id: PeerId = "12D3KooWRelayPeerId...".parse()?; // 👈 Replace with actual
    let relay_addr: Multiaddr = "/ip4/1.2.3.4/tcp/4001".parse()?;   // 👈 Replace with actual

    // Add the relay to our routing table
    swarm
        .behaviour_mut()
        .relay
        .add_address(&relay_peer_id, relay_addr.clone());

    // Friend's peer ID
    let friend_peer_id: PeerId = "12D3KooWNode2PeerId...".parse()?; // 👈 Replace with actual

    // Build the full relay circuit address to Node 2
    let circuit_addr = relay_addr
        .with(libp2p::multiaddr::Protocol::P2p(relay_peer_id.into()))
        .with(libp2p::multiaddr::Protocol::P2pCircuit)
        .with(libp2p::multiaddr::Protocol::P2p(friend_peer_id.into()));

    // Dial Node 2 via the relay
    swarm.dial(circuit_addr)?;

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::Behaviour(PingEvent { peer, .. }) => {
                println!("Ping with {}", peer);
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {:?}", address);
            }
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                if let Some(err) = cause {
                    println!("Connection with {} closed due to error: {:?}", peer_id, err);
                } else {
                    println!("Connection with {} closed gracefully", peer_id);
                }
            }
            other => {
                println!("Unhandled swarm event: {:?}", other);
            }
        }
    }
}
