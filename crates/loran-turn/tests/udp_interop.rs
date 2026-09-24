use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::prelude::BASE64_STANDARD;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use turn::{Authenticator, create_turn_server, shutdown_turn_server};
use turn_crate::client::{Client, ClientConfig};
use webrtc_util::Conn as _;

const REALM: &str = "interop.actrix.test";
const TURN_SECRET: &str = "turn-interop-secret-at-least-32-bytes";

fn scoped_credential() -> (String, String) {
    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after the Unix epoch")
        .as_secs()
        + 300;
    let username = format!(
        "{expires_at}:realm=k7m2q9xh@example.com:provider=provider.example:audience=actr.turn:actor=1.k7m2q9xh@example.com"
    );
    let secret = platform::realm::realm_turn_secret(
        TURN_SECRET.as_bytes(),
        "k7m2q9xh@example.com",
        "provider.example",
    );
    let mut mac = Hmac::<Sha1>::new_from_slice(&secret).expect("HMAC accepts this secret");
    mac.update(username.as_bytes());
    let password = BASE64_STANDARD.encode(mac.finalize().into_bytes());
    (username, password)
}

async fn start_turn_server() -> anyhow::Result<(turn_crate::server::Server, String)> {
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let address = socket.local_addr()?.to_string();
    let authenticator = Arc::new(Authenticator::new(
        TURN_SECRET.to_string(),
        "provider.example".to_string(),
    )?);
    let server = create_turn_server(
        socket,
        "127.0.0.1",
        REALM,
        (49_152, 65_535),
        authenticator,
        true,
    )
    .await?;
    Ok((server, address))
}

async fn turn_client(server_address: &str) -> anyhow::Result<Client> {
    let (username, password) = scoped_credential();
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let client = Client::new(ClientConfig {
        stun_serv_addr: server_address.to_string(),
        turn_serv_addr: server_address.to_string(),
        username,
        password,
        realm: REALM.to_string(),
        software: "actrix-turn-interop-test".to_string(),
        rto_in_ms: 50,
        conn: socket,
        vnet: None,
    })
    .await?;
    client.listen().await?;
    Ok(client)
}

#[tokio::test]
async fn udp_allocate_permission_and_bidirectional_relay_interoperate() -> anyhow::Result<()> {
    let (server, server_address) = start_turn_server().await?;
    let client = turn_client(&server_address).await?;

    let mapped_address = timeout(Duration::from_secs(3), client.send_binding_request()).await??;
    assert_eq!(mapped_address.ip().to_string(), "127.0.0.1");

    let relay = timeout(Duration::from_secs(3), client.allocate()).await??;
    let relay_address = relay.local_addr()?;
    assert_eq!(relay_address.ip().to_string(), "127.0.0.1");
    assert!((49_152..=65_535).contains(&relay_address.port()));

    let peer = UdpSocket::bind("127.0.0.1:0").await?;
    let peer_address = peer.local_addr()?;
    relay.send_to(b"client-to-peer", peer_address).await?;

    let mut peer_buffer = [0_u8; 64];
    let (peer_len, peer_source) =
        timeout(Duration::from_secs(3), peer.recv_from(&mut peer_buffer)).await??;
    assert_eq!(&peer_buffer[..peer_len], b"client-to-peer");
    assert_eq!(peer_source, relay_address);

    peer.send_to(b"peer-to-client", relay_address).await?;
    let mut relay_buffer = [0_u8; 64];
    let (relay_len, relay_source) =
        timeout(Duration::from_secs(3), relay.recv_from(&mut relay_buffer)).await??;
    assert_eq!(&relay_buffer[..relay_len], b"peer-to-client");
    assert_eq!(relay_source, peer_address);

    relay.close().await?;
    client.close().await?;
    shutdown_turn_server(&server).await?;
    Ok(())
}

#[tokio::test]
async fn malformed_udp_burst_does_not_starve_a_valid_allocation() -> anyhow::Result<()> {
    let (server, server_address) = start_turn_server().await?;
    let abuse_socket = UdpSocket::bind("127.0.0.1:0").await?;

    // Alternate malformed STUN-looking packets and ChannelData-looking packets.
    // This is a bounded abuse regression, not a capacity benchmark.
    for index in 0..4_096 {
        let mut packet = [0_u8; 32];
        packet[0] = if index % 2 == 0 { 0x00 } else { 0x40 };
        abuse_socket.send_to(&packet, &server_address).await?;
    }

    let client = turn_client(&server_address).await?;
    let relay = timeout(Duration::from_secs(5), client.allocate()).await??;
    assert!((49_152..=65_535).contains(&relay.local_addr()?.port()));

    relay.close().await?;
    client.close().await?;
    shutdown_turn_server(&server).await?;
    Ok(())
}
