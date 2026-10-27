//! Public HTTPS document fetches made by a privileged process.
//!
//! Authority descriptors, Realm Profiles and publisher documents are fetched
//! from URLs that other parties choose. Each hop is HTTPS only, resolves to
//! public addresses only, is pinned to the checked address, and the response
//! is limited to 64 KiB of JSON, so the fetch cannot be turned into an SSRF
//! primitive (`actr/protocols/realm-profile.md` section 3).

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use url::{Host, Url};

/// Largest accepted document.
pub const MAX_DOCUMENT_BYTES: usize = 64 * 1024;
/// Most redirects followed; every hop is checked again.
pub const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct FetchError(String);

impl FetchError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

/// Addresses a fetch must never reach: loopback, private, link-local,
/// shared, multicast and reserved ranges, including their IPv6 mappings.
pub fn is_forbidden_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || a == 0
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 198 && (18..=19).contains(&b))
        }
        IpAddr::V6(ip) => {
            // IPv4-mapped/compatible addresses connect to IPv4 destinations at
            // the socket layer, so they must be judged by the IPv4 rules;
            // otherwise ::ffff:169.254.169.254 etc. would bypass the blacklist.
            if let Some(v4) = ip.to_ipv4_mapped().or_else(|| ip.to_ipv4()) {
                return is_forbidden_address(IpAddr::V4(v4));
            }
            let seg = ip.segments();
            ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || seg[0] == 0x2002 // 6to4 (2002::/16)
                || (seg[0] == 0x0064 && seg[1] == 0xff9b) // NAT64 well-known 64:ff9b::/96
        }
    }
}

/// Test-only relaxations: trust a local CA and allow loopback.
#[derive(Clone, Default)]
struct Trust {
    #[cfg(test)]
    root: Option<reqwest::Certificate>,
    #[cfg(test)]
    loopback: bool,
}

impl Trust {
    fn allows(&self, ip: IpAddr) -> bool {
        #[cfg(test)]
        if self.loopback && ip.is_loopback() {
            return true;
        }
        !is_forbidden_address(ip)
    }
}

/// Fetch a public JSON document with `GET`, following at most
/// `max_redirects` redirects. Each hop must be HTTPS without user info and
/// resolve only to public addresses; the request is pinned to the checked
/// address so DNS rebinding cannot redirect it.
pub async fn fetch_public_json(url: &str, max_redirects: usize) -> Result<Vec<u8>, FetchError> {
    fetch(url, max_redirects, &Trust::default()).await
}

async fn fetch(url: &str, max_redirects: usize, trust: &Trust) -> Result<Vec<u8>, FetchError> {
    let mut url =
        Url::parse(url).map_err(|error| FetchError::new(format!("invalid URL: {error}")))?;
    for _ in 0..=max_redirects {
        let client = pinned_client(&url, trust).await?;
        let response = client
            .get(url.clone())
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| FetchError::new(format!("HTTPS fetch of {url} failed: {error}")))?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| FetchError::new(format!("redirect from {url} has no Location")))?;
            url = url
                .join(location)
                .map_err(|error| FetchError::new(format!("invalid redirect target: {error}")))?;
            continue;
        }
        return read_json(response).await;
    }
    Err(FetchError::new(format!(
        "more than {max_redirects} redirects"
    )))
}

async fn pinned_client(url: &Url, trust: &Trust) -> Result<reqwest::Client, FetchError> {
    if url.scheme() != "https" {
        return Err(FetchError::new(format!("{url} is not HTTPS")));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(FetchError::new(format!("{url} carries user info")));
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let (domain, addresses) = match url.host() {
        Some(Host::Domain(domain)) => {
            let mut addresses = tokio::net::lookup_host((domain, port))
                .await
                .map_err(|error| {
                    FetchError::new(format!("DNS lookup of {domain} failed: {error}"))
                })?
                .map(|address| address.ip())
                .collect::<Vec<_>>();
            addresses.sort_unstable();
            addresses.dedup();
            (Some(domain), addresses)
        }
        Some(Host::Ipv4(ip)) => (None, vec![IpAddr::V4(ip)]),
        Some(Host::Ipv6(ip)) => (None, vec![IpAddr::V6(ip)]),
        None => return Err(FetchError::new(format!("{url} has no host"))),
    };
    if addresses.is_empty() {
        return Err(FetchError::new(format!("{url} has no address records")));
    }
    if !addresses.iter().all(|ip| trust.allows(*ip)) {
        return Err(FetchError::new(format!(
            "{url} resolves to a non-public address"
        )));
    }

    // Bound every phase of the request so a slow endpoint cannot hang the
    // fetch. `.no_proxy()` keeps an ambient proxy from bypassing the pinned
    // address and the address check above.
    let mut builder = reqwest::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15));
    if let Some(domain) = domain {
        builder = builder.resolve(domain, SocketAddr::new(addresses[0], port));
    }
    #[cfg(test)]
    if let Some(root) = &trust.root {
        builder = builder.add_root_certificate(root.clone());
    }
    builder
        .build()
        .map_err(|error| FetchError::new(error.to_string()))
}

async fn read_json(mut response: reqwest::Response) -> Result<Vec<u8>, FetchError> {
    let url = response.url().clone();
    if response.status() != reqwest::StatusCode::OK {
        return Err(FetchError::new(format!(
            "{url} returned HTTP {}",
            response.status()
        )));
    }
    let is_json = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("application/json")
        });
    if !is_json {
        return Err(FetchError::new(format!(
            "{url} is not served as application/json"
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DOCUMENT_BYTES as u64)
    {
        return Err(FetchError::new(format!("{url} exceeds 64 KiB")));
    }
    // Chunked responses have no Content-Length; enforce the cap while reading.
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| FetchError::new(error.to_string()))?
    {
        if bytes.len() + chunk.len() > MAX_DOCUMENT_BYTES {
            return Err(FetchError::new(format!("{url} exceeds 64 KiB")));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

type Cached = (Instant, Result<Arc<[u8]>, FetchError>);

/// [`fetch_public_json`] with a cache that also keeps failures, and a limit
/// on fetches per host per minute. Fetches that happen before any signature
/// is checked go through here so a caller cannot use them to hammer a host.
pub struct CachedFetcher {
    ttl: Duration,
    per_host_per_minute: u32,
    trust: Trust,
    state: Mutex<FetcherState>,
}

#[derive(Default)]
struct FetcherState {
    cache: HashMap<String, Cached>,
    hosts: HashMap<String, (Instant, u32)>,
}

/// Bound on remembered URLs and hosts.
const MAX_ENTRIES: usize = 4096;

impl Default for CachedFetcher {
    fn default() -> Self {
        Self::new(Duration::from_secs(300), 10)
    }
}

impl CachedFetcher {
    pub fn new(ttl: Duration, per_host_per_minute: u32) -> Self {
        Self {
            ttl,
            per_host_per_minute,
            trust: Trust::default(),
            state: Mutex::default(),
        }
    }

    /// Fetch `url` (at most [`MAX_REDIRECTS`] redirects), answering from the
    /// cache while an earlier result, success or failure, is fresh.
    pub async fn get(&self, url: &str) -> Result<Arc<[u8]>, FetchError> {
        let host = Url::parse(url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .ok_or_else(|| FetchError::new(format!("invalid URL: {url}")))?;
        {
            let now = Instant::now();
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((at, result)) = state.cache.get(url)
                && now.duration_since(*at) < self.ttl
            {
                return result.clone();
            }
            if state.cache.len() >= MAX_ENTRIES {
                let ttl = self.ttl;
                state
                    .cache
                    .retain(|_, (at, _)| now.duration_since(*at) < ttl);
            }
            if !state.hosts.contains_key(&host) && state.hosts.len() >= MAX_ENTRIES {
                state
                    .hosts
                    .retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(60));
                // Reclaiming expired windows may leave the table full. Refuse
                // new hosts under this same lock instead of exceeding the cap
                // or evicting an active host and resetting its rate limit.
                if state.hosts.len() >= MAX_ENTRIES {
                    return Err(FetchError::new("fetch host table is full"));
                }
            }
            let window = state.hosts.entry(host.clone()).or_insert((now, 0));
            if now.duration_since(window.0) >= Duration::from_secs(60) {
                *window = (now, 0);
            }
            if window.1 >= self.per_host_per_minute {
                return Err(FetchError::new(format!("too many fetches from {host}")));
            }
            window.1 += 1;
        }

        let result = fetch(url, MAX_REDIRECTS, &self.trust).await.map(Arc::from);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.cache.len() < MAX_ENTRIES {
            state
                .cache
                .insert(url.to_string(), (Instant::now(), result.clone()));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn rejects_private_addresses() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.0.1",
            "169.254.1.1",
            "100.64.0.1",
            "::1",
            "fd00::1",
            "fe80::1",
            // IPv4-mapped / compatible IPv6 that reach forbidden IPv4 targets.
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::ffff:10.0.0.1",
            "::7f00:1",
            // 6to4 and NAT64 well-known prefixes.
            "2002::1",
            "64:ff9b::a9fe:a9fe",
        ] {
            assert!(
                is_forbidden_address(ip.parse().unwrap()),
                "{ip} should be forbidden"
            );
        }
        assert!(!is_forbidden_address("1.1.1.1".parse().unwrap()));
        assert!(!is_forbidden_address(
            "2606:4700:4700::1111".parse().unwrap()
        ));
    }

    #[tokio::test]
    async fn rejects_unsafe_urls_before_connecting() {
        for url in [
            "http://example.com/.well-known/actr",
            "https://user:pw@example.com/.well-known/actr",
            "https://127.0.0.1/.well-known/actr",
            "https://[::1]/.well-known/actr",
            "https://localhost/.well-known/actr",
            "https://10.1.2.3:8443/x",
            "not a url",
        ] {
            assert!(
                fetch_public_json(url, MAX_REDIRECTS).await.is_err(),
                "{url}"
            );
        }
    }

    /// A TLS server on loopback answering `respond(path, port)` with a
    /// status line, content type and body.
    async fn serve(
        respond: fn(&str, u16) -> (String, &'static str, Vec<u8>),
    ) -> (u16, Trust, Arc<AtomicUsize>) {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let cert = certified.cert.der().clone();
        let key =
            rustls_pki_types::PrivateKeyDer::Pkcs8(certified.signing_key.serialize_der().into());
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key)
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let acceptor = acceptor.clone();
                let counter = counter.clone();
                tokio::spawn(async move {
                    let Ok(mut stream) = acceptor.accept(stream).await else {
                        return;
                    };
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 1024];
                    while !request.ends_with(b"\r\n\r\n") {
                        match stream.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => request.extend_from_slice(&buffer[..n]),
                        }
                    }
                    counter.fetch_add(1, Ordering::SeqCst);
                    let request = String::from_utf8_lossy(&request);
                    let path = request.split_whitespace().nth(1).unwrap_or("/");
                    let (status, content_type, body) = respond(path, port);
                    let head = format!(
                        "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\n\
                         content-length: {}\r\nconnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(&body).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        let trust = Trust {
            root: Some(reqwest::Certificate::from_der(&cert).unwrap()),
            loopback: true,
        };
        (port, trust, hits)
    }

    fn routes(path: &str, port: u16) -> (String, &'static str, Vec<u8>) {
        let redirect = |location: String| {
            (
                format!("302 Found\r\nlocation: {location}"),
                "text/plain",
                Vec::new(),
            )
        };
        match path {
            "/doc" => (
                "200 OK".into(),
                "application/json",
                br#"{"ok":true}"#.to_vec(),
            ),
            "/relative" => redirect("/doc".into()),
            "/absolute" => redirect(format!("https://localhost:{port}/doc")),
            "/plain" => redirect(format!("http://localhost:{port}/doc")),
            "/private" => redirect("https://10.0.0.1/doc".into()),
            "/big" => (
                "200 OK".into(),
                "application/json",
                vec![b' '; MAX_DOCUMENT_BYTES + 1],
            ),
            "/html" => ("200 OK".into(), "text/html", b"<html></html>".to_vec()),
            "/missing" => ("404 Not Found".into(), "application/json", b"{}".to_vec()),
            path => match path
                .strip_prefix("/loop/")
                .and_then(|n| n.parse::<u32>().ok())
            {
                Some(0) => redirect("/doc".into()),
                Some(n) => redirect(format!("/loop/{}", n - 1)),
                None => ("404 Not Found".into(), "text/plain", Vec::new()),
            },
        }
    }

    #[tokio::test]
    async fn follows_checked_redirects_and_limits_documents() {
        let (port, trust, _) = serve(routes).await;
        let url = |path: &str| format!("https://localhost:{port}{path}");

        for path in ["/doc", "/relative", "/absolute", "/loop/4"] {
            let body = fetch(&url(path), MAX_REDIRECTS, &trust).await.unwrap();
            assert_eq!(body, br#"{"ok":true}"#, "{path}");
        }
        // /loop/4 takes five redirects; /loop/5 takes six.
        for path in ["/loop/5", "/plain", "/private", "/big", "/html", "/missing"] {
            let error = fetch(&url(path), MAX_REDIRECTS, &trust).await.unwrap_err();
            assert!(!error.to_string().is_empty(), "{path}");
        }
        assert!(fetch(&url("/relative"), 0, &trust).await.is_err());
        // Without the test relaxation loopback is refused.
        assert!(fetch_public_json(&url("/doc"), 0).await.is_err());
    }

    #[tokio::test]
    async fn caches_results_and_limits_each_host() {
        let (port, trust, hits) = serve(routes).await;
        let url = |path: &str| format!("https://localhost:{port}{path}");
        let fetcher = CachedFetcher {
            trust,
            ..CachedFetcher::new(Duration::from_secs(60), 3)
        };

        assert!(fetcher.get(&url("/doc")).await.is_ok());
        assert!(fetcher.get(&url("/doc")).await.is_ok());
        assert!(fetcher.get(&url("/missing")).await.is_err());
        assert!(fetcher.get(&url("/missing")).await.is_err());
        assert_eq!(hits.load(Ordering::SeqCst), 2, "results are cached");

        assert!(fetcher.get(&url("/relative")).await.is_ok());
        let error = fetcher.get(&url("/html")).await.unwrap_err();
        assert!(error.to_string().contains("too many fetches"), "{error}");
    }

    #[tokio::test]
    async fn host_admission_caps_fresh_hosts_and_preserves_existing_windows() {
        let fetcher = CachedFetcher::new(Duration::from_secs(300), 2);
        // HTTP is rejected locally before DNS/network I/O. Failed fetches still
        // consume a host slot, just like failed public descriptor lookups.
        for i in 0..MAX_ENTRIES + 8 {
            let url = format!("http://host-{i}.example/doc");
            let error = fetcher.get(&url).await.unwrap_err();
            let expected = if i < MAX_ENTRIES {
                "is not HTTPS"
            } else {
                "fetch host table is full"
            };
            assert!(error.to_string().contains(expected), "host {i}: {error}");
        }
        {
            let state = fetcher.state.lock().unwrap();
            assert_eq!(state.hosts.len(), MAX_ENTRIES);
            assert_eq!(state.cache.len(), MAX_ENTRIES);
            assert!(
                !state
                    .hosts
                    .contains_key(&format!("host-{MAX_ENTRIES}.example"))
            );
        }
        // A full table must neither block known hosts nor reset their budget.
        let error = fetcher
            .get("http://host-0.example/second")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("is not HTTPS"), "{error}");
        let error = fetcher
            .get("http://host-0.example/third")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("too many fetches"), "{error}");

        // Reclaim an expired window before deciding whether a new host fits.
        fetcher
            .state
            .lock()
            .unwrap()
            .hosts
            .get_mut("host-0.example")
            .unwrap()
            .0 = Instant::now() - Duration::from_secs(61);
        let error = fetcher
            .get("http://replacement.example/doc")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("is not HTTPS"), "{error}");
        let state = fetcher.state.lock().unwrap();
        assert_eq!(state.hosts.len(), MAX_ENTRIES);
        assert!(!state.hosts.contains_key("host-0.example"));
        assert!(state.hosts.contains_key("replacement.example"));
    }
}
