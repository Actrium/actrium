// Shared by test binaries only. Replace virtual-domain document transport,
// never document signatures, management authorization or membership handlers.
pub async fn fetch(url: String) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        url.starts_with("https://localhost/") || url.starts_with("https://tenant.realm-e2e.test/"),
        "unexpected Realm E2E document URL: {url}"
    );
    let path = std::env::var("ACTR_REALM_E2E_DOCUMENTS")?;
    let documents: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let document = documents
        .get(&url)
        .ok_or_else(|| anyhow::anyhow!("Realm E2E document not published: {url}"))?;
    Ok(serde_json::to_vec(document)?)
}
