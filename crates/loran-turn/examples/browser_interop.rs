//! Local Chromium/WebRTC interoperability fixture for Actrix TURN.
//!
//! Run with:
//! `cargo run -p actrix-turn --example browser_interop`
//! and open the printed localhost URL in Chromium. The page forces both peers
//! to use relay candidates and reports the selected candidate pair.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::prelude::BASE64_STANDARD;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use turn::{Authenticator, create_turn_server, shutdown_turn_server};
use uuid::Uuid;

const REALM: &str = "browser-interop.actrix.test";
const ACTOR_ID: &str = "1.k7m2q9xh@example.com";

fn scoped_credential(secret: &str) -> (String, String) {
    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after the Unix epoch")
        .as_secs()
        + 600;
    let username = format!(
        "{expires_at}:realm=k7m2q9xh@example.com:provider=provider.example:audience=actr.turn:actor={ACTOR_ID}"
    );
    let mut mac =
        Hmac::<Sha1>::new_from_slice(secret.as_bytes()).expect("HMAC accepts this secret");
    mac.update(username.as_bytes());
    let password = BASE64_STANDARD.encode(mac.finalize().into_bytes());
    (username, password)
}

fn page(turn_address: &str, username: &str, password: &str) -> String {
    let turn_url = serde_json::to_string(&format!("turn:{turn_address}?transport=udp"))
        .expect("serialize TURN URL");
    let username = serde_json::to_string(username).expect("serialize username");
    let password = serde_json::to_string(password).expect("serialize password");
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Actrix TURN browser interoperability</title>
</head>
<body>
  <h1>Actrix TURN browser interoperability</h1>
  <pre id="result">running</pre>
  <script>
    const result = document.getElementById("result");
    const report = (value) => {{
      window.__actrTurnInterop = value;
      result.textContent = JSON.stringify(value, null, 2);
    }};
    const withTimeout = (promise, ms, name) => Promise.race([
      promise,
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error(`${{name}} timed out after ${{ms}} ms`)), ms)),
    ]);
    const waitForGathering = (pc) => pc.iceGatheringState === "complete"
      ? Promise.resolve()
      : new Promise((resolve) => {{
          pc.addEventListener("icegatheringstatechange", () => {{
            if (pc.iceGatheringState === "complete") resolve();
          }});
        }});
    const selectedPair = async (pc) => {{
      const stats = await pc.getStats();
      let pair;
      for (const stat of stats.values()) {{
        if (stat.type === "transport" && stat.selectedCandidatePairId) {{
          pair = stats.get(stat.selectedCandidatePairId);
          break;
        }}
      }}
      if (!pair) {{
        for (const stat of stats.values()) {{
          if (stat.type === "candidate-pair" && stat.state === "succeeded" && stat.nominated) {{
            pair = stat;
            break;
          }}
        }}
      }}
      if (!pair) return null;
      const local = stats.get(pair.localCandidateId);
      const remote = stats.get(pair.remoteCandidateId);
      return {{
        state: pair.state,
        localCandidateType: local?.candidateType,
        remoteCandidateType: remote?.candidateType,
        protocol: local?.protocol,
      }};
    }};

    (async () => {{
      const configuration = {{
        iceServers: [{{
          urls: [{turn_url}],
          username: {username},
          credential: {password},
        }}],
        iceTransportPolicy: "relay",
      }};
      const left = new RTCPeerConnection(configuration);
      const right = new RTCPeerConnection(configuration);
      const remoteChannel = new Promise((resolve) => {{
        right.ondatachannel = (event) => resolve(event.channel);
      }});
      const localChannel = left.createDataChannel("actrix-turn-interop");

      const offer = await left.createOffer();
      await left.setLocalDescription(offer);
      await withTimeout(waitForGathering(left), 10_000, "offer ICE gathering");
      await right.setRemoteDescription(left.localDescription);

      const answer = await right.createAnswer();
      await right.setLocalDescription(answer);
      await withTimeout(waitForGathering(right), 10_000, "answer ICE gathering");
      await left.setRemoteDescription(right.localDescription);

      const incoming = await withTimeout(remoteChannel, 10_000, "remote data channel");
      incoming.onmessage = (event) => incoming.send(`pong:${{event.data}}`);
      await withTimeout(new Promise((resolve) => {{
        if (localChannel.readyState === "open") resolve();
        else localChannel.onopen = resolve;
      }}), 10_000, "local data channel");

      const reply = new Promise((resolve) => {{
        localChannel.onmessage = (event) => resolve(event.data);
      }});
      localChannel.send("ping");
      const message = await withTimeout(reply, 5_000, "relayed data");
      const pair = await selectedPair(left);
      const relayCandidates = (left.localDescription?.sdp.match(/ typ relay/g) || []).length
        + (right.localDescription?.sdp.match(/ typ relay/g) || []).length;

      if (message !== "pong:ping") throw new Error(`unexpected reply: ${{message}}`);
      if (!pair || pair.localCandidateType !== "relay" || pair.remoteCandidateType !== "relay") {{
        throw new Error(`selected pair is not relay-only: ${{JSON.stringify(pair)}}`);
      }}
      report({{ status: "passed", message, relayCandidates, selectedPair: pair }});
      left.close();
      right.close();
    }})().catch((error) => report({{ status: "failed", error: String(error?.stack || error) }}));
  </script>
</body>
</html>
"#
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let turn_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let turn_address = turn_socket.local_addr()?.to_string();
    let secret = format!("browser-interop-{}-{}", Uuid::new_v4(), Uuid::new_v4());
    let (username, password) = scoped_credential(&secret);
    let authenticator = Arc::new(Authenticator::new(secret, "provider.example".to_string())?);
    let turn_server = create_turn_server(
        turn_socket,
        "127.0.0.1",
        REALM,
        (49_152, 65_535),
        authenticator,
        // Loopback interop fixture: relay between local addresses is expected.
        true,
    )
    .await?;

    let http_listener = TcpListener::bind("127.0.0.1:0").await?;
    let http_address = http_listener.local_addr()?;
    let body = Arc::new(page(&turn_address, &username, &password));
    println!("ACTR_TURN_BROWSER_URL=http://{http_address}/");
    println!("ACTR_TURN_UDP_ADDRESS={turn_address}");

    let serve = async move {
        loop {
            let (mut stream, _) = http_listener.accept().await?;
            let body = Arc::clone(&body);
            tokio::spawn(async move {
                let mut request = [0_u8; 2_048];
                let _ = stream.read(&mut request).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
        #[allow(unreachable_code)]
        Ok::<(), std::io::Error>(())
    };

    tokio::select! {
        result = serve => result?,
        result = tokio::signal::ctrl_c() => result?,
    }
    shutdown_turn_server(&turn_server).await?;
    Ok(())
}
