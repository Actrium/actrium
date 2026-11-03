//! STUN server implementation
//!
//! Provides STUN protocol server functionality for NAT discovery and traversal.
#![deny(clippy::disallowed_macros)]

pub mod error;

// Re-export error types for convenience
pub use error::{ErrorSeverity, Result, StunError};

/// Optional external observation hook. Protocol code has no metrics backend.
#[async_trait::async_trait]
pub trait RequestObserver: Send + Sync {
    async fn on_request(&self, success: bool);
}
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
pub use turn_crate::server::request::is_stun_datagram as is_stun_message;

/// Create and run a STUN server with graceful shutdown support
pub async fn create_stun_server_with_shutdown(
    socket: Arc<UdpSocket>,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    create_stun_server_with_shutdown_and_counters(socket, shutdown_rx, None).await
}

/// Create and run a STUN server with graceful shutdown support and optional service counters.
pub async fn create_stun_server_with_shutdown_and_counters(
    socket: Arc<UdpSocket>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    counters: Option<Arc<dyn RequestObserver>>,
) -> Result<()> {
    tracing::info!(
        "Starting STUN server with shutdown support on {}",
        socket.local_addr()?
    );

    let mut buffer = vec![0u8; turn_crate::proto::UDP_RECEIVE_BUFFER_SIZE];

    loop {
        tokio::select! {
            // Handle incoming UDP packets
            result = socket.recv_from(&mut buffer) => {
                match result {
                    Ok((len, src_addr)) => {
                        let packet_data = &buffer[..len];

                        // Check if this might be a STUN message before processing
                        if is_stun_message(packet_data) {
                            tracing::debug!("Received potential STUN packet from {} ({} bytes)", src_addr, len);

                            // Parse and respond inline to bound work under unauthenticated UDP floods.
                            let parsed_ok = match process_packet(socket.clone(), packet_data, src_addr).await {
                                Ok(parsed_ok) => parsed_ok,
                                Err(e) => {
                                    tracing::error!("Failed to process STUN packet from {}: {}", src_addr, e);
                                    false
                                }
                            };
                            if let Some(ref ctr) = counters {
                                ctr.on_request(parsed_ok).await;
                            }
                        } else {
                            tracing::debug!("Received non-STUN packet from {} ({} bytes), ignoring", src_addr, len);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error receiving UDP packet: {}", e);
                        return Err(e.into());
                    }
                }
            }

            // Handle shutdown signal
            _ = shutdown_rx.recv() => {
                tracing::info!("Received shutdown signal, stopping STUN server");
                break;
            }
        }
    }

    tracing::info!("STUN server has been shut down");
    Ok(())
}

/// Process a Binding datagram with the shared upstream protocol implementation.
pub async fn process_packet(socket: Arc<UdpSocket>, data: &[u8], src: SocketAddr) -> Result<bool> {
    let mut message = webrtc_stun::message::Message::new();
    if message.write(data).is_err()
        || message.raw.len() != 20 + message.length as usize
        || !message.length.is_multiple_of(4)
    {
        return Ok(false);
    }
    turn_crate::server::request::handle_binding_datagram(socket, data, src)
        .await
        .map_err(|error| StunError::PacketSendFailed {
            reason: error.to_string(),
        })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use webrtc_stun::attributes::*;
    use webrtc_stun::error_code::{CODE_UNKNOWN_ATTRIBUTE, ErrorCodeAttribute};
    use webrtc_stun::fingerprint::FINGERPRINT;
    use webrtc_stun::message::{BINDING_SUCCESS, Message};
    use webrtc_stun::uattrs::UnknownAttributes;
    // use webrtc_stun::client::ClientBuilder; // Unused in current tests
    use anyhow::Result;
    // Make process_packet and handle_binding_request available
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::net::UdpSocket;
    use tokio::time::timeout;
    use webrtc_stun::agent::TransactionId;
    use webrtc_stun::message::{BINDING_REQUEST, Getter as _};
    use webrtc_stun::xoraddr::XorMappedAddress;

    #[tokio::test]
    async fn unknown_required_attributes_and_invalid_fingerprints() -> Result<()> {
        let server = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
        let client = UdpSocket::bind("127.0.0.1:0").await?;
        let mut request = Message::new();
        request.build(&[Box::new(TransactionId::new()), Box::new(BINDING_REQUEST)])?;
        request.add(AttrType(0x1234), &[0; 4]);
        process_packet(server.clone(), &request.raw, client.local_addr()?).await?;
        let mut buffer = [0; 2048];
        let (len, _) = timeout(Duration::from_secs(1), client.recv_from(&mut buffer)).await??;
        let mut response = Message::new();
        response.write(&buffer[..len])?;
        let mut error = ErrorCodeAttribute::default();
        error.get_from(&response)?;
        assert_eq!(error.code.0, CODE_UNKNOWN_ATTRIBUTE.0);
        let mut unknown = UnknownAttributes(vec![]);
        unknown.get_from(&response)?;
        assert_eq!(unknown.0, vec![AttrType(0x1234)]);
        assert_eq!(response.transaction_id, request.transaction_id);
        let mut invalid = Message::new();
        invalid.build(&[
            Box::new(TransactionId::new()),
            Box::new(BINDING_REQUEST),
            Box::new(FINGERPRINT),
        ])?;
        let last = invalid.raw.len() - 1;
        invalid.raw[last] ^= 1;
        process_packet(server.clone(), &invalid.raw, client.local_addr()?).await?;
        assert!(
            timeout(Duration::from_millis(40), client.recv_from(&mut buffer))
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_stun_packet_processing_works() -> Result<()> {
        // Setup a "server" socket that our process_packet will use to send responses
        let server_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
        let server_addr = server_socket.local_addr()?;

        // Setup a "client" socket
        let client_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
        let client_addr = client_socket.local_addr()?;
        client_socket.connect(server_addr).await?; // Client sends to server_addr

        // Client: Craft a STUN Binding Request message
        let mut request_msg = Message::new();
        request_msg.build(&[Box::<TransactionId>::default(), Box::new(BINDING_REQUEST)])?;

        // Client: Send the request to the server socket
        client_socket.send(&request_msg.raw).await?;

        // Server side: receive the packet and process it
        let mut recv_buf = [0; 1024];
        let (len, src_addr) = timeout(
            Duration::from_secs(1),
            server_socket.recv_from(&mut recv_buf),
        )
        .await??;
        assert_eq!(src_addr, client_addr);

        // Call our STUN packet processor
        process_packet(server_socket.clone(), &recv_buf[..len], src_addr).await?;

        // Client: Wait for the response
        let (response_len, _) = timeout(
            Duration::from_secs(1),
            client_socket.recv_from(&mut recv_buf),
        )
        .await??;

        let mut response_stun_msg = Message::new();
        response_stun_msg.write(&recv_buf[..response_len])?;

        // Verify the response
        assert_eq!(response_stun_msg.typ, BINDING_SUCCESS);
        assert_eq!(response_stun_msg.transaction_id, request_msg.transaction_id);

        let mut xor_addr = XorMappedAddress::default();
        xor_addr.get_from(&response_stun_msg)?;

        assert_eq!(xor_addr.ip, client_addr.ip());
        assert_eq!(xor_addr.port, client_addr.port());

        Ok(())
    }

    #[tokio::test]
    async fn test_is_stun_message() {
        // Valid STUN Binding Request (first two bits 00)
        let mut msg = Message::new();
        msg.build(&[Box::<TransactionId>::default(), Box::new(BINDING_REQUEST)])
            .unwrap();
        assert!(is_stun_message(&msg.raw));

        // Not a STUN message (e.g., first two bits 01 - ChannelData)
        let not_stun_data = [0x40, 0x01, 0x00, 0x00]; // Example ChannelData prefix
        assert!(!is_stun_message(&not_stun_data));

        // Empty data
        assert!(!is_stun_message(&[]));

        // Too short but looks like STUN
        // is_stun_message only checks the first byte, full parsing is in process_packet
        let short_stun_data = [0x00, 0x01];
        assert!(is_stun_message(&short_stun_data));
    }

    #[tokio::test]
    async fn test_stun_server_with_shutdown() -> Result<()> {
        use std::time::Duration;
        use tokio::sync::broadcast;
        use tokio::time::timeout;

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Setup server socket
        let server_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
        let server_addr = server_socket.local_addr()?;

        // Start the STUN server in background
        let server_socket_clone = server_socket.clone();
        let server_handle = tokio::spawn(async move {
            create_stun_server_with_shutdown(server_socket_clone, shutdown_rx).await
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Setup client socket and send request
        let client_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
        client_socket.connect(server_addr).await?;

        // Create and send STUN binding request
        let mut request_msg = Message::new();
        request_msg.build(&[Box::<TransactionId>::default(), Box::new(BINDING_REQUEST)])?;
        client_socket.send(&request_msg.raw).await?;

        // Receive response
        let mut recv_buf = [0; 1024];
        let (response_len, _) = timeout(
            Duration::from_secs(1),
            client_socket.recv_from(&mut recv_buf),
        )
        .await??;

        // Parse and verify response
        let mut response_stun_msg = Message::new();
        response_stun_msg.write(&recv_buf[..response_len])?;
        assert_eq!(response_stun_msg.typ, BINDING_SUCCESS);

        // Test shutdown
        let _ = shutdown_tx.send(());

        // Wait for server to shut down
        let _ = timeout(Duration::from_secs(1), server_handle).await??;

        Ok(())
    }

    #[tokio::test]
    async fn malformed_udp_burst_does_not_starve_binding_requests() -> Result<()> {
        use tokio::sync::broadcast;

        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let server_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
        let server_addr = server_socket.local_addr()?;
        let server_handle = tokio::spawn(create_stun_server_with_shutdown(
            Arc::clone(&server_socket),
            shutdown_rx,
        ));

        let abuse_socket = UdpSocket::bind("127.0.0.1:0").await?;
        let abuse_handle = tokio::spawn(async move {
            for index in 0..4_096 {
                let mut packet = [0_u8; 32];
                packet[0] = if index % 2 == 0 { 0x00 } else { 0x40 };
                abuse_socket.send_to(&packet, server_addr).await?;
                if index % 128 == 0 {
                    tokio::task::yield_now().await;
                }
            }
            Ok::<(), std::io::Error>(())
        });

        let client_socket = UdpSocket::bind("127.0.0.1:0").await?;
        let client_addr = client_socket.local_addr()?;
        let mut response_buffer = [0_u8; 1_024];
        let mut received = None;

        // UDP may legitimately drop packets under pressure, so retry the valid
        // RFC 5389 request while the malformed burst is in flight.
        for _ in 0..20 {
            let mut request = Message::new();
            request.build(&[Box::<TransactionId>::default(), Box::new(BINDING_REQUEST)])?;
            client_socket.send_to(&request.raw, server_addr).await?;

            if let Ok(Ok((length, source))) = timeout(
                Duration::from_millis(100),
                client_socket.recv_from(&mut response_buffer),
            )
            .await
            {
                let mut response = Message::new();
                response.write(&response_buffer[..length])?;
                received = Some((request, response, source));
                break;
            }
        }

        abuse_handle.await??;
        let (request, response, source) =
            received.expect("valid STUN Binding request must not be starved");
        assert_eq!(source, server_addr);
        assert_eq!(response.typ, BINDING_SUCCESS);
        assert_eq!(response.transaction_id, request.transaction_id);
        let mut mapped = XorMappedAddress::default();
        mapped.get_from(&response)?;
        assert_eq!(mapped.ip, client_addr.ip());
        assert_eq!(mapped.port, client_addr.port());

        let _ = shutdown_tx.send(());
        timeout(Duration::from_secs(1), server_handle).await???;
        Ok(())
    }
}
