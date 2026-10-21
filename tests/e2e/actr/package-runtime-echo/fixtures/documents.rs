// Shared by test binaries only. HTTPS mode keeps TLS/hostname verification;
// the explicit private test CA and restricted loopback tunnel are not compiled
// into production. The map-only mode remains for the legacy rotation fixture.
pub async fn fetch(url: String) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        url.starts_with("https://localhost/")
            || url.starts_with("https://tenant.realm-e2e.test/")
            || url == "https://e2e.actrium.test/.well-known/actr",
        "unexpected Realm E2E document URL: {url}"
    );
    if std::env::var_os("ACTR_REALM_E2E_HTTPS_CONFIG").is_some() {
        return fetch_https(&url, 64 * 1024).await;
    }
    let path = std::env::var("ACTR_REALM_E2E_DOCUMENTS")?;
    let documents: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let document = documents
        .get(&url)
        .ok_or_else(|| anyhow::anyhow!("Realm E2E document not published: {url}"))?;
    Ok(serde_json::to_vec(document)?)
}

pub async fn fetch_https(url: &str, limit: usize) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(url.starts_with("https://"), "HTTPS is required");
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(std::env::var(
        "ACTR_REALM_E2E_HTTPS_CONFIG",
    )?)?)?;
    let ca = std::fs::read(
        config["ca_file"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("ca_file"))?,
    )?;
    let proxy = config["proxy"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("proxy"))?;
    let proxy_url = url::Url::parse(proxy)?;
    anyhow::ensure!(
        proxy_url.scheme() == "http"
            && proxy_url.host_str() == Some("127.0.0.1")
            && proxy_url.port().is_some()
            && proxy_url.username().is_empty()
            && proxy_url.password().is_none(),
        "test proxy must be loopback"
    );
    let client = reqwest::Client::builder()
        .https_only(true)
        .tls_built_in_root_certs(false)
        .add_root_certificate(reqwest::Certificate::from_pem(&ca)?)
        .proxy(reqwest::Proxy::all(proxy)?)
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut response = client.get(url).send().await?.error_for_status()?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            body.len() + chunk.len() <= limit,
            "HTTPS body exceeds limit"
        );
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
